use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

use ring::rand::SystemRandom;
use ring::signature::Ed25519KeyPair;
use rusqlite::{params, Connection};
use skycode_runtime::approval::token::{public_key_bytes, ApprovalToken};
use skycode_runtime::approval::validator::register_signing_key;
use skycode_runtime::db::events::{append_event, content_id, EventType, ToolEvent};
use skycode_runtime::db::migrations::run_migrations;
use skycode_runtime::tools::apply::apply_diff;
use skycode_runtime::tools::diff::DiffProposal;
use tempfile::TempDir;

#[test]
fn phase7_api_approve_apply_roundtrip() -> Result<(), Box<dyn std::error::Error>> {
    let temp = TempDir::new()?;
    let repo = create_temp_git_repo(&temp)?;
    let conn = migrated_conn(&temp)?;
    let agent_id = "coder-primary";
    let task_id = "task-phase7-api";
    let project_id = "default";
    let (key_pair, public_key) = make_keypair()?;
    let public_key_hex = public_key
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect::<String>();

    register_signing_key(&conn, agent_id, &public_key_hex, unix_now()?)?;

    let diff = DiffProposal {
        id: uuid::Uuid::new_v4(),
        project_id: project_id.to_string(),
        diff_text: patch_for("hello.txt", "hello", "api-world"),
        file_path: "hello.txt".to_string(),
        created_at: unix_now()?,
    };
    let token = ApprovalToken::create_signed(
        project_id,
        diff.id.to_string(),
        agent_id,
        agent_id,
        "phase7-api-nonce",
        &key_pair,
    )?;

    apply_diff(&conn, &token, agent_id, task_id, &repo, project_id, &diff)?;

    let content = fs::read_to_string(repo.join("hello.txt"))?;
    assert_eq!(content.replace("\r\n", "\n"), "api-world\n");

    let used_count: i64 = conn.query_row(
        "SELECT COUNT(*) FROM approval_tokens_used WHERE tid = ?1 AND diff_id = ?2",
        params![token.id.to_string(), diff.id.to_string()],
        |row| row.get(0),
    )?;
    assert_eq!(used_count, 1);

    append_event(
        &conn,
        &ToolEvent {
            id: content_id(format!("phase7-api:{}:{}", token.id, diff.id).as_bytes()),
            task_id: task_id.to_string(),
            agent_id: agent_id.to_string(),
            event_type: EventType::DiffApplied,
            tool_name: Some("apply_diff".to_string()),
            inputs_hash: None,
            inputs_json: None,
            output_hash: None,
            output_json: None,
            approval_token_id: Some(token.id.to_string()),
            diff_id: Some(diff.id.to_string()),
            profile_name: None,
            created_at: unix_now()?,
        },
    )?;

    let event_count: i64 = conn.query_row(
        "SELECT COUNT(*) FROM tool_events
         WHERE event_type = 'diff_applied'
           AND approval_token_id = ?1
           AND diff_id = ?2",
        params![token.id.to_string(), diff.id.to_string()],
        |row| row.get(0),
    )?;
    assert_eq!(event_count, 1);

    Ok(())
}

#[test]
fn phase7_api_layer_boundary() -> Result<(), Box<dyn std::error::Error>> {
    let repo_root = Path::new(env!("CARGO_MANIFEST_DIR")).join("..");

    for dir in ["api/src", "mcp/src"] {
        for path in rust_files(&repo_root.join(dir))? {
            let content = fs::read_to_string(&path)?;
            assert!(
                !content.contains("UPDATE "),
                "{} must not contain raw UPDATE SQL",
                path.display()
            );
            assert!(
                !content.contains("DELETE FROM"),
                "{} must not contain raw DELETE SQL",
                path.display()
            );
        }
    }

    Ok(())
}

fn migrated_conn(temp: &TempDir) -> Result<Connection, Box<dyn std::error::Error>> {
    let conn = Connection::open(temp.path().join("phase7-api.db"))?;
    run_migrations(&conn, &migrations_dir())?;
    Ok(conn)
}

fn rust_files(dir: &Path) -> Result<Vec<PathBuf>, Box<dyn std::error::Error>> {
    let mut files = Vec::new();
    collect_rust_files(dir, &mut files)?;
    Ok(files)
}

fn collect_rust_files(
    dir: &Path,
    files: &mut Vec<PathBuf>,
) -> Result<(), Box<dyn std::error::Error>> {
    for entry in fs::read_dir(dir)? {
        let entry = entry?;
        let path = entry.path();
        if path.is_dir() {
            collect_rust_files(&path, files)?;
        } else if path.extension().and_then(|ext| ext.to_str()) == Some("rs") {
            files.push(path);
        }
    }
    Ok(())
}

fn make_keypair() -> Result<(Ed25519KeyPair, Vec<u8>), Box<dyn std::error::Error>> {
    let rng = SystemRandom::new();
    let pkcs8 =
        Ed25519KeyPair::generate_pkcs8(&rng).map_err(|_| "failed to generate Ed25519 key")?;
    let key_pair =
        Ed25519KeyPair::from_pkcs8(pkcs8.as_ref()).map_err(|_| "failed to parse Ed25519 key")?;
    let public_key = public_key_bytes(&key_pair);
    Ok((key_pair, public_key))
}

fn create_temp_git_repo(temp: &TempDir) -> Result<PathBuf, Box<dyn std::error::Error>> {
    let repo = temp.path().join("repo");
    fs::create_dir_all(&repo)?;
    run_git(&repo, &["init"])?;
    run_git(&repo, &["config", "core.autocrlf", "false"])?;
    run_git(&repo, &["config", "core.eol", "lf"])?;
    run_git(&repo, &["config", "user.email", "phase7-api@skycode.local"])?;
    run_git(&repo, &["config", "user.name", "phase7-api"])?;
    fs::write(repo.join("hello.txt"), "hello\n")?;
    run_git(&repo, &["add", "."])?;
    run_git(&repo, &["commit", "-m", "baseline"])?;
    Ok(repo)
}

fn patch_for(path: &str, before: &str, after: &str) -> String {
    format!(
        "diff --git a/{path} b/{path}\n--- a/{path}\n+++ b/{path}\n@@ -1 +1 @@\n-{before}\n+{after}\n"
    )
}

fn run_git(repo: &Path, args: &[&str]) -> Result<(), Box<dyn std::error::Error>> {
    let output = Command::new("git")
        .arg("-C")
        .arg(repo)
        .args(args)
        .output()?;
    if !output.status.success() {
        return Err(format!(
            "git failed: git -C {} {}\n{}",
            repo.display(),
            args.join(" "),
            String::from_utf8_lossy(&output.stderr)
        )
        .into());
    }
    Ok(())
}

fn migrations_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("memory")
        .join("migrations")
}

fn unix_now() -> Result<i64, Box<dyn std::error::Error>> {
    Ok(i64::try_from(
        SystemTime::now().duration_since(UNIX_EPOCH)?.as_secs(),
    )?)
}
