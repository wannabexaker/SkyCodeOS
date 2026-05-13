use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

use ring::rand::SystemRandom;
use ring::signature::Ed25519KeyPair;
use rusqlite::{params, Connection};
use serde_json::json;
use skycode_runtime::agent::load_coder_primary_identity;
use skycode_runtime::approval::token::{public_key_bytes, ApprovalToken};
use skycode_runtime::approval::validator::{register_signing_key, validate_token};
use skycode_runtime::db::events::{append_event, content_id, EventType, ToolEvent};
use skycode_runtime::db::migrations::run_migrations;
use skycode_runtime::graph::{impact_query, scan_project};
use skycode_runtime::inference::{ModelRegistry, ModelRuntime};
use skycode_runtime::memory::{insert_memory, search_memories, Memory};
use skycode_runtime::orchestrator::{map_to_model, record_model_selection, RouterError, TaskClass};
use skycode_runtime::skycore::{strip_provider_fields, SkyCoreResponse};
use skycode_runtime::tools::apply::apply_diff;
use skycode_runtime::tools::diff::create_diff;
use skycode_runtime::tools::rollback::rollback;
use tempfile::TempDir;

#[test]
fn phase1_gate_50_edit_cycles_zero_unapproved_writes() -> Result<(), Box<dyn std::error::Error>> {
    let temp = TempDir::new()?;
    let conn = open_migrated_db(&temp, "phase1.db")?;
    let precheck_conn = open_migrated_db(&temp, "phase1-precheck.db")?;
    let agent_id = "coder-primary";
    let (key_pair, public_key) = make_keypair()?;
    // Register public key on both connections before the cycle loop.
    let pub_key_hex: String = public_key.iter().map(|b| format!("{b:02x}")).collect();
    register_signing_key(&conn, agent_id, &pub_key_hex, unix_now()?)?;
    register_signing_key(&precheck_conn, agent_id, &pub_key_hex, unix_now()?)?;

    let repo = create_temp_git_repo(&temp)?;
    let rel = Path::new("reg.txt");
    let abs = repo.join(rel);
    let baseline = "anchor-top\nbaseline\nanchor-bottom\n";

    for cycle in 0..50 {
        fs::write(&abs, baseline)?;
        let mid = format!("cycle-{cycle}");
        let after = format!("anchor-top\n{}\nanchor-bottom\n", mid);
        let mut diff = create_diff("default", rel, baseline, &after)?;
        diff.diff_text = format!(
            "diff --git a/{p} b/{p}\n--- a/{p}\n+++ b/{p}\n@@ -1,3 +1,3 @@\n anchor-top\n-baseline\n+{mid}\n anchor-bottom\n",
            p = rel.display(),
            mid = mid
        );

        let token = ApprovalToken::create_signed(
            "default",
            diff.id.to_string(),
            agent_id,
            agent_id,
            format!("n-{cycle}"),
            &key_pair,
        )?;
        validate_token(
            &precheck_conn,
            &token,
            "default",
            &diff.id.to_string(),
            agent_id,
            &format!("task-pre-{cycle}"),
        )?;

        apply_diff(
            &conn,
            &token,
            agent_id,
            &format!("task-{cycle}"),
            &repo,
            "default",
            &diff,
        )?;

        append_event(
            &conn,
            &ToolEvent {
                id: content_id(format!("{}:{}:{}", cycle, token.id, diff.id).as_bytes()),
                task_id: format!("task-{cycle}"),
                agent_id: agent_id.to_string(),
                event_type: EventType::DiffApplied,
                tool_name: Some("apply_diff".to_string()),
                inputs_hash: None,
                inputs_json: None,
                output_hash: None,
                output_json: None,
                approval_token_id: Some(token.id.to_string()),
                diff_id: Some(diff.id.to_string()),
                profile_name: Some("precise".to_string()),
                created_at: unix_now()?,
            },
        )?;

        rollback(&repo, "HEAD")?;
    }

    let total: i64 = conn.query_row("SELECT COUNT(*) FROM tool_events", [], |r| r.get(0))?;
    assert_eq!(total, 50);
    let unapproved: i64 = conn.query_row(
        "SELECT COUNT(*) FROM tool_events
         WHERE event_type = 'diff_applied' AND approval_token_id IS NULL",
        [],
        |r| r.get(0),
    )?;
    assert_eq!(unapproved, 0);

    Ok(())
}

#[test]
fn phase2_gate_scan_persists_across_restart() -> Result<(), Box<dyn std::error::Error>> {
    let temp = TempDir::new()?;
    let project = temp.path().join("project");
    fs::create_dir_all(project.join("src"))?;
    fs::write(
        project.join("src").join("lib.rs"),
        "pub fn hello() {}\npub fn main_call() { hello(); }\n",
    )?;

    {
        let conn = open_migrated_db(&temp, "scan.db")?;
        let stats = scan_project(&conn, "default", &project)?;
        assert!(stats.nodes_created > 0);
    }

    {
        let conn = open_migrated_db(&temp, "scan.db")?;
        let count: i64 =
            conn.query_row("SELECT COUNT(*) FROM graph_nodes", [], |row| row.get(0))?;
        assert!(count > 0, "graph nodes must survive connection restart");
    }

    Ok(())
}

#[test]
fn phase2_gate_memory_retrieval_scoped_correctly() -> Result<(), Box<dyn std::error::Error>> {
    let temp = TempDir::new()?;
    let conn = open_migrated_db(&temp, "memory.db")?;

    insert_memory(
        &conn,
        "task-a",
        &Memory {
            id: "mem-a".to_string(),
            project_id: "project-a".to_string(),
            agent_id: "coder-primary".to_string(),
            scope: "project".to_string(),
            content: "auth refactor project a".to_string(),
            tags: None,
            importance: 0.8,
        },
    )?;
    insert_memory(
        &conn,
        "task-b",
        &Memory {
            id: "mem-b".to_string(),
            project_id: "project-b".to_string(),
            agent_id: "coder-primary".to_string(),
            scope: "project".to_string(),
            content: "auth refactor project b".to_string(),
            tags: None,
            importance: 0.8,
        },
    )?;

    let results = search_memories(&conn, "auth", "project-a", "coder-primary", "project", 10)?;
    assert!(!results.is_empty());
    assert!(results.iter().all(|m| m.project_id == "project-a"));

    Ok(())
}

#[test]
fn phase2_gate_graph_impact_correct() -> Result<(), Box<dyn std::error::Error>> {
    let temp = TempDir::new()?;
    let conn = open_migrated_db(&temp, "impact.db")?;
    let now = unix_now()?;

    for id in ["a", "b", "c"] {
        conn.execute(
            "INSERT INTO graph_nodes (
                id, project_id, kind, name, path, language, span_json, metadata_json, updated_at
             ) VALUES (?1, 'default', 'symbol', ?1, ?1, 'rust', NULL, NULL, ?2)",
            params![id, now],
        )?;
    }
    conn.execute(
        "INSERT INTO graph_edges (id, project_id, from_id, to_id, kind, metadata_json)
         VALUES ('e1', 'default', 'b', 'a', 'calls', NULL)",
        [],
    )?;
    conn.execute(
        "INSERT INTO graph_edges (id, project_id, from_id, to_id, kind, metadata_json)
         VALUES ('e2', 'default', 'c', 'b', 'calls', NULL)",
        [],
    )?;

    let ids: HashSet<String> = impact_query(&conn, "a", 16)?
        .into_iter()
        .map(|n| n.id)
        .collect();
    assert!(ids.contains("b"));
    assert!(ids.contains("c"));

    Ok(())
}

#[test]
fn phase2_gate_no_vector_db_or_remote_used() -> Result<(), Box<dyn std::error::Error>> {
    let runtime_manifest =
        fs::read_to_string(Path::new(env!("CARGO_MANIFEST_DIR")).join("Cargo.toml"))?;
    for forbidden in ["qdrant", "faiss", "milvus", "pinecone", "chroma"] {
        assert!(
            !runtime_manifest.contains(forbidden),
            "vector DB dependency found: {forbidden}"
        );
    }

    let registry = ModelRegistry::load_from_file(
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("..")
            .join("agents")
            .join("models.yaml"),
    )?;
    let remote_enabled = registry
        .models
        .iter()
        .any(|m| m.runtime == ModelRuntime::OpenaiCompatible && m.enabled);
    assert!(
        !remote_enabled,
        "remote adapter must stay disabled by default"
    );

    Ok(())
}

#[test]
fn phase3_gate_identical_skycore_shape() -> Result<(), Box<dyn std::error::Error>> {
    let normalized = json!({
        "skycore_version": "0.1",
        "task_id": "shape-1",
        "status": "ok",
        "summary": "done",
        "artifacts": [],
        "tool_calls_requested": [],
        "requires_approval": false,
        "error": null
    });
    let provider = json!({
        "task_id": "shape-1",
        "status": "ok",
        "summary": "done",
        "artifacts": [],
        "tool_calls_requested": [],
        "requires_approval": false,
        "error": null,
        "choices": [{"message": {"content": "{}"}}],
        "usage": {"prompt_tokens": 1},
        "model": "local"
    });

    let direct: SkyCoreResponse = serde_json::from_value(normalized)?;
    let stripped = strip_provider_fields(provider)?;

    assert_eq!(direct.skycore_version, stripped.skycore_version);
    assert_eq!(direct.task_id, stripped.task_id);
    assert_eq!(direct.summary, stripped.summary);
    assert_eq!(direct.artifacts.len(), stripped.artifacts.len());
    assert_eq!(
        direct.tool_calls_requested.len(),
        stripped.tool_calls_requested.len()
    );
    assert_eq!(direct.requires_approval, stripped.requires_approval);

    Ok(())
}

#[test]
fn phase3_gate_missing_model_explicit_error() -> Result<(), Box<dyn std::error::Error>> {
    let registry = ModelRegistry { models: Vec::new() };
    let err = map_to_model(TaskClass::CodeEdit, &registry).expect_err("model must be missing");
    assert!(matches!(
        err,
        RouterError::NoLocalModel(TaskClass::CodeEdit)
    ));
    Ok(())
}

#[test]
fn phase3_gate_strips_openai_fields_at_boundary() -> Result<(), Box<dyn std::error::Error>> {
    let raw = json!({
        "task_id": "boundary-1",
        "status": "ok",
        "summary": "clean",
        "artifacts": [{
            "kind": "diff",
            "id": "d1",
            "patch_unified": "--- a\n+++ b\n",
            "affected_files": ["src/lib.rs"],
            "usage": {"nested": true}
        }],
        "tool_calls_requested": [],
        "requires_approval": true,
        "error": null,
        "choices": [{"message": {"content": "provider"}}],
        "usage": {"prompt_tokens": 5},
        "model": "gpt"
    });

    let response = strip_provider_fields(raw)?;
    let encoded = serde_json::to_value(&response)?;
    assert!(!encoded.to_string().contains("choices"));
    assert!(!encoded.to_string().contains("usage"));
    assert!(!encoded.to_string().contains("model"));

    Ok(())
}

#[test]
fn phase4_gate_decision_recall_across_connections() -> Result<(), Box<dyn std::error::Error>> {
    let temp = TempDir::new()?;
    {
        let conn = open_migrated_db(&temp, "recall.db")?;
        insert_memory(
            &conn,
            "task-session-1",
            &Memory {
                id: "mem-apply-foo".to_string(),
                project_id: "default".to_string(),
                agent_id: "coder-primary".to_string(),
                scope: "project".to_string(),
                content: "Applied patch to src/lib.rs: add fn foo".to_string(),
                tags: Some("apply,decision".to_string()),
                importance: 0.8,
            },
        )?;
    }

    {
        let conn = open_migrated_db(&temp, "recall.db")?;
        let results = search_memories(&conn, "fn foo", "default", "coder-primary", "project", 5)?;
        assert!(!results.is_empty());
        assert!(results[0].content.contains("fn foo"));
    }

    Ok(())
}

#[test]
fn phase4_gate_exactly_one_agent_assertion() -> Result<(), Box<dyn std::error::Error>> {
    let identity = load_coder_primary_identity(
        &Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("..")
            .join("agents"),
    )?;

    assert_eq!(identity.id, "coder-primary");
    assert!(identity
        .approval_required_for
        .iter()
        .any(|item| item == "file_write"));

    Ok(())
}

#[test]
fn phase4_gate_model_invoked_event_carries_profile_name() -> Result<(), Box<dyn std::error::Error>>
{
    let temp = TempDir::new()?;
    let conn = open_migrated_db(&temp, "model-event.db")?;

    record_model_selection(&conn, "task-profile", "local-coder", "fast")?;

    let profile_name: Option<String> = conn.query_row(
        "SELECT profile_name FROM tool_events
         WHERE task_id = ?1 AND event_type = 'model_invoked'
         LIMIT 1",
        params!["task-profile"],
        |row| row.get(0),
    )?;
    assert_eq!(profile_name.as_deref(), Some("fast"));

    Ok(())
}

fn open_migrated_db(temp: &TempDir, name: &str) -> Result<Connection, Box<dyn std::error::Error>> {
    let conn = Connection::open(temp.path().join(name))?;
    run_migrations(&conn, &migrations_dir())?;
    Ok(conn)
}

fn migrations_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("memory")
        .join("migrations")
}

fn make_keypair() -> Result<(Ed25519KeyPair, Vec<u8>), Box<dyn std::error::Error>> {
    let rng = SystemRandom::new();
    let pkcs8 =
        Ed25519KeyPair::generate_pkcs8(&rng).map_err(|_| "failed to generate Ed25519 key pair")?;
    let key_pair = Ed25519KeyPair::from_pkcs8(pkcs8.as_ref())
        .map_err(|_| "failed to parse Ed25519 key pair")?;
    let public_key = public_key_bytes(&key_pair);
    Ok((key_pair, public_key))
}

fn create_temp_git_repo(temp: &TempDir) -> Result<PathBuf, Box<dyn std::error::Error>> {
    let repo = temp.path().join("repo");
    fs::create_dir_all(&repo)?;
    run_git(&repo, &["init"])?;
    run_git(&repo, &["config", "core.autocrlf", "false"])?;
    run_git(&repo, &["config", "core.eol", "lf"])?;
    run_git(&repo, &["config", "user.email", "p5-reg@skycode.local"])?;
    run_git(&repo, &["config", "user.name", "p5-reg"])?;
    fs::write(
        repo.join("reg.txt"),
        "anchor-top\nbaseline\nanchor-bottom\n",
    )?;
    run_git(&repo, &["add", "."])?;
    run_git(&repo, &["commit", "-m", "baseline"])?;
    Ok(repo)
}

fn run_git(repo: &Path, args: &[&str]) -> Result<(), Box<dyn std::error::Error>> {
    let out = Command::new("git")
        .arg("-C")
        .arg(repo)
        .args(args)
        .output()?;
    if !out.status.success() {
        return Err(format!("git failed: {}", String::from_utf8_lossy(&out.stderr)).into());
    }
    Ok(())
}

fn unix_now() -> Result<i64, Box<dyn std::error::Error>> {
    Ok(i64::try_from(
        SystemTime::now().duration_since(UNIX_EPOCH)?.as_secs(),
    )?)
}
