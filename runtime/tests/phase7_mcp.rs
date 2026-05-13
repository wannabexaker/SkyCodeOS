use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::{Arc, Mutex};
use std::time::{SystemTime, UNIX_EPOCH};

use ring::rand::SystemRandom;
use ring::signature::Ed25519KeyPair;
use rusqlite::{params, Connection};
use serde_json::{json, Value};
use skycode_mcp::dispatch::dispatch_tool;
use skycode_mcp::handler::handle_request;
use skycode_mcp::proto::RpcRequest;
use skycode_mcp::state::McpState;
use skycode_runtime::db::migrations::run_migrations;
use tempfile::TempDir;

#[test]
fn phase7_mcp_list_tools() -> Result<(), Box<dyn std::error::Error>> {
    let temp = TempDir::new()?;
    let state = mcp_state(temp.path())?;
    let response = handle_request(
        RpcRequest {
            jsonrpc: "2.0".to_string(),
            id: Some(json!(1)),
            method: "tools/list".to_string(),
            params: None,
        },
        &state,
    )
    .expect("tools/list must return a response");

    let result = response.result.expect("tools/list must return result");
    let tools = result["tools"]
        .as_array()
        .expect("tools/list result must contain tools array");
    let names = tools
        .iter()
        .map(|tool| tool["name"].as_str().unwrap_or_default())
        .collect::<Vec<_>>();

    assert_eq!(
        names,
        vec![
            "list_models",
            "get_agent_state",
            "get_diff",
            "search_memory",
            "approve_diff",
            "apply_diff",
            "apply_diff_set",
            "run_verify",
        ]
    );

    for tool in tools {
        assert!(
            !tool["description"].as_str().unwrap_or_default().is_empty(),
            "tool descriptions must be non-empty"
        );
        assert_eq!(tool["inputSchema"]["type"], "object");
    }

    Ok(())
}

#[test]
fn phase7_mcp_readonly_no_auth() -> Result<(), Box<dyn std::error::Error>> {
    let temp = TempDir::new()?;
    let state = mcp_state(temp.path())?;

    let result = dispatch_tool("list_models", json!({}), &state);

    assert_ne!(result["isError"], true);
    assert!(result["content"].is_array());

    Ok(())
}

#[test]
fn phase7_mcp_mutate_requires_key() -> Result<(), Box<dyn std::error::Error>> {
    let temp = TempDir::new()?;
    let state = mcp_state(temp.path())?;

    let result = dispatch_tool("run_verify", json!({ "api_key": "wrong" }), &state);

    assert_eq!(result["isError"], true);
    assert!(result["content"][0]["text"]
        .as_str()
        .unwrap_or_default()
        .contains("Unauthorized"));

    Ok(())
}

#[test]
fn phase7_mcp_apply_roundtrip() -> Result<(), Box<dyn std::error::Error>> {
    let temp = TempDir::new()?;
    let repo = create_temp_git_repo(&temp)?;
    write_signing_key(&repo)?;

    let state = mcp_state(&repo)?;
    let diff_id = uuid::Uuid::new_v4().to_string();
    let patch = patch_for("hello.txt", "hello", "world");
    {
        let conn = state.conn.lock().map_err(|_| "database lock poisoned")?;
        insert_diff(
            &conn,
            &diff_id,
            "task-phase7",
            "coder-primary",
            "default",
            &patch,
            "hello.txt",
        )?;
    }

    let approve = call_tool(
        "approve_diff",
        json!({
            "api_key": "secret",
            "diff_id": diff_id,
            "agent_id": "coder-primary",
            "task_id": "task-phase7"
        }),
        &state,
    );
    assert_ne!(approve["isError"], true, "approve_diff failed: {approve}");
    let token_text = approve["content"][0]["text"]
        .as_str()
        .ok_or("approve_diff must return text content")?;
    let token: Value = serde_json::from_str(token_text)?;

    let apply = call_tool(
        "apply_diff",
        json!({
            "api_key": "secret",
            "diff_id": diff_id,
            "token": token,
            "agent_id": "coder-primary",
            "task_id": "task-phase7"
        }),
        &state,
    );
    assert_ne!(apply["isError"], true, "apply_diff failed: {apply}");

    let content = fs::read_to_string(repo.join("hello.txt"))?;
    assert_eq!(content.replace("\r\n", "\n"), "world\n");

    let used_count: i64 = state
        .conn
        .lock()
        .map_err(|_| "database lock poisoned")?
        .query_row(
            "SELECT COUNT(*) FROM approval_tokens_used WHERE diff_id = ?1",
            params![diff_id],
            |row| row.get(0),
        )?;
    assert_eq!(used_count, 1);

    Ok(())
}

fn call_tool(name: &str, arguments: Value, state: &McpState) -> Value {
    handle_request(
        RpcRequest {
            jsonrpc: "2.0".to_string(),
            id: Some(json!(1)),
            method: "tools/call".to_string(),
            params: Some(json!({ "name": name, "arguments": arguments })),
        },
        state,
    )
    .and_then(|response| response.result)
    .expect("tools/call must return result")
}

fn mcp_state(project_root: &Path) -> Result<McpState, Box<dyn std::error::Error>> {
    let db_path = project_root.join("skycode.db");
    let conn = Connection::open(&db_path)?;
    run_migrations(&conn, &migrations_dir())?;

    Ok(McpState {
        project_root: project_root.to_path_buf(),
        models_yaml_path: Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("..")
            .join("agents")
            .join("models.yaml"),
        db_path,
        api_key: Arc::new("secret".to_string()),
        conn: Arc::new(Mutex::new(conn)),
    })
}

fn insert_diff(
    conn: &Connection,
    diff_id: &str,
    task_id: &str,
    agent_id: &str,
    project_id: &str,
    patch: &str,
    file_path: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    let now = unix_now()?;
    conn.execute(
        "INSERT INTO diff_proposals (
            id, task_id, agent_id, project_id, patch_unified, base_git_ref,
            base_blob_hashes_json, affected_files_json, created_at, expires_at
         ) VALUES (?1, ?2, ?3, ?4, ?5, 'HEAD', '{}', ?6, ?7, NULL)",
        params![
            diff_id,
            task_id,
            agent_id,
            project_id,
            patch,
            json!([file_path]).to_string(),
            now,
        ],
    )?;
    Ok(())
}

fn write_signing_key(repo: &Path) -> Result<(), Box<dyn std::error::Error>> {
    let keys_dir = repo.join(".skycode").join("keys");
    fs::create_dir_all(&keys_dir)?;
    let rng = SystemRandom::new();
    let pkcs8 =
        Ed25519KeyPair::generate_pkcs8(&rng).map_err(|_| "failed to generate Ed25519 key")?;
    fs::write(keys_dir.join("approval_ed25519.pk8"), pkcs8.as_ref())?;
    Ok(())
}

fn create_temp_git_repo(temp: &TempDir) -> Result<PathBuf, Box<dyn std::error::Error>> {
    let repo = temp.path().join("repo");
    fs::create_dir_all(&repo)?;
    run_git(&repo, &["init"])?;
    run_git(&repo, &["config", "core.autocrlf", "false"])?;
    run_git(&repo, &["config", "core.eol", "lf"])?;
    run_git(&repo, &["config", "user.email", "phase7@skycode.local"])?;
    run_git(&repo, &["config", "user.name", "phase7"])?;
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
