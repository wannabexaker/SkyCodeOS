//! Phase 11 - daily-use safety guards.

use std::fs;
use std::path::{Path, PathBuf};

use rusqlite::Connection;
use skycode_runtime::db::migrations::run_migrations;
use skycode_runtime::orchestrator::{run_task_loop, OrchestratorError, TaskLoopInput};
use tempfile::TempDir;

#[test]
fn phase11_destructive_rewrite_refused_by_default() -> Result<(), Box<dyn std::error::Error>> {
    let temp = TempDir::new()?;
    let repo = setup_repo(&temp)?;
    write_file(&repo, "docs/clients.md", &numbered_lines("original", 100))?;
    write_mock_response(&repo, "docs/clients.md", &numbered_lines("replacement", 5))?;
    let conn = migrated_conn(&temp)?;

    let result = run_task_loop(&conn, &input(&repo, false));

    match result {
        Err(OrchestratorError::DestructiveDiff { removed, original }) => {
            assert_eq!(original, 100);
            assert!(removed > 50, "expected a large deletion, got {removed}");
        }
        other => panic!("expected destructive diff rejection, got {other:?}"),
    }

    let content = fs::read_to_string(repo.join("docs").join("clients.md"))?;
    assert_eq!(content, numbered_lines("original", 100));
    Ok(())
}

#[test]
fn phase11_destructive_rewrite_allowed_with_flag() -> Result<(), Box<dyn std::error::Error>> {
    let temp = TempDir::new()?;
    let repo = setup_repo(&temp)?;
    write_file(&repo, "docs/clients.md", &numbered_lines("original", 100))?;
    write_mock_response(&repo, "docs/clients.md", &numbered_lines("replacement", 5))?;
    let conn = migrated_conn(&temp)?;

    let output = run_task_loop(&conn, &input(&repo, true))?;
    assert!(output.diff.diff_text.contains("docs/clients.md"));

    let stored: i64 =
        conn.query_row("SELECT COUNT(*) FROM diff_proposals", [], |row| row.get(0))?;
    assert_eq!(stored, 1);
    Ok(())
}

#[test]
fn phase11_additive_change_passes_guard() -> Result<(), Box<dyn std::error::Error>> {
    let temp = TempDir::new()?;
    let repo = setup_repo(&temp)?;
    let original = numbered_lines("original", 100);
    let new_content = format!("intro one\nintro two\nintro three\n{original}");
    write_file(&repo, "docs/clients.md", &original)?;
    write_mock_response(&repo, "docs/clients.md", &new_content)?;
    let conn = migrated_conn(&temp)?;

    let output = run_task_loop(&conn, &input(&repo, false))?;

    assert!(output.diff.diff_text.contains("+intro one"));
    assert!(output.diff.diff_text.contains("+intro three"));
    Ok(())
}

#[test]
fn phase11_small_file_with_full_rewrite_passes() -> Result<(), Box<dyn std::error::Error>> {
    let temp = TempDir::new()?;
    let repo = setup_repo(&temp)?;
    write_file(&repo, "docs/clients.md", &numbered_lines("small", 3))?;
    write_mock_response(&repo, "docs/clients.md", &numbered_lines("replacement", 10))?;
    let conn = migrated_conn(&temp)?;

    let output = run_task_loop(&conn, &input(&repo, false))?;

    assert!(output.diff.diff_text.contains("docs/clients.md"));
    Ok(())
}

fn setup_repo(temp: &TempDir) -> Result<PathBuf, Box<dyn std::error::Error>> {
    let repo = temp.path().join("repo");
    fs::create_dir_all(&repo)?;
    write_agent_identity(&repo)?;
    Ok(repo)
}

fn write_agent_identity(repo: &Path) -> Result<(), Box<dyn std::error::Error>> {
    let agents = repo.join("agents").join("coder-primary").join("core");
    fs::create_dir_all(&agents)?;
    fs::write(
        agents.join("soul.yaml"),
        "id: coder-primary\nname: Coder Primary\nrole: persistent_coder\ncore_values:\n  - correctness\n",
    )?;
    fs::write(
        agents.join("doctrine.yaml"),
        "must_never:\n  - write_without_approval\nmust_always:\n  - produce_diff_before_apply\napproval_required_for:\n  - file_write\n",
    )?;
    fs::write(
        agents.join("heart.yaml"),
        "communication_style: concise\nerror_handling: fail_visible\n",
    )?;
    fs::write(
        agents.join("mind.yaml"),
        "planning_depth: shallow_task_level\nrisk_tolerance: low\n",
    )?;
    Ok(())
}

fn write_file(repo: &Path, rel: &str, content: &str) -> Result<(), Box<dyn std::error::Error>> {
    let path = repo.join(rel);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(path, content)?;
    Ok(())
}

fn write_mock_response(
    repo: &Path,
    rel: &str,
    new_content: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    let skycode_dir = repo.join(".skycode");
    fs::create_dir_all(&skycode_dir)?;
    let response = serde_json::json!({
        "skycore_version": "0.1",
        "task_id": "phase11-task",
        "status": "ok",
        "summary": "Updated docs",
        "artifacts": [{
            "kind": "rewrite",
            "id": rel,
            "new_content": new_content,
            "affected_files": [rel]
        }],
        "tool_calls_requested": [],
        "requires_approval": true,
        "error": null
    });
    fs::write(
        skycode_dir.join("mock_model_response.json"),
        serde_json::to_string_pretty(&response)?,
    )?;
    Ok(())
}

fn input(repo: &Path, allow_destructive: bool) -> TaskLoopInput {
    TaskLoopInput {
        task_id: "phase11-task".to_string(),
        project_id: "phase11-project".to_string(),
        goal: "Add a short paragraph at the top of docs/clients.md".to_string(),
        repo_root: repo.to_string_lossy().to_string(),
        profile: "precise".to_string(),
        allow_destructive,
    }
}

fn migrated_conn(temp: &TempDir) -> Result<Connection, Box<dyn std::error::Error>> {
    let conn = Connection::open(temp.path().join("phase11.db"))?;
    run_migrations(&conn, &migrations_dir())?;
    Ok(conn)
}

fn migrations_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("memory")
        .join("migrations")
}

fn numbered_lines(prefix: &str, count: usize) -> String {
    (1..=count)
        .map(|n| format!("{prefix} line {n:03}\n"))
        .collect()
}
