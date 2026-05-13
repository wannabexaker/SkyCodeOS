use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use clap::Args;
use rusqlite::{params, Connection, OptionalExtension};
use serde_json::json;
use uuid::Uuid;

use skycode_orchestrator::db::events::{append_event, content_id, EventType, ToolEvent};
use skycode_orchestrator::db::migrations::run_migrations;
use skycode_orchestrator::memory::store::{insert_memory, Decision, Memory};
use skycode_orchestrator::orchestrator::write_decision;
use skycode_orchestrator::tools::apply::apply_diff;
use skycode_orchestrator::tools::diff::DiffProposal;
use skycode_tools::tools::verify::run_verify;

use crate::commands::approve::{load_token, remove_token};
#[derive(Debug, Args)]
pub struct ApplyArgs {
    /// The UUID of the diff proposal to apply (requires prior approval).
    pub diff_id: String,

    /// Path to repository root where patch should be applied.
    #[arg(long, default_value = ".")]
    pub repo: PathBuf,

    /// Run the configured test_command after apply. Exits 2 if tests fail or time out.
    #[arg(long)]
    pub verify: bool,
}

pub fn run(args: &ApplyArgs) -> Result<(), Box<dyn std::error::Error>> {
    let conn = open_db_with_migrations()?;
    let applied = apply_diff_from_store_with_conn(&conn, &args.diff_id, &args.repo)?;

    println!("Applied diff: {}", args.diff_id);

    if args.verify {
        run_post_apply_verify(&conn, &args.repo, &applied.task_id, &args.diff_id)?;
    }

    Ok(())
}

pub fn apply_diff_from_store(
    diff_id: &str,
    repo_path: &Path,
) -> Result<(), Box<dyn std::error::Error>> {
    let conn = open_db_with_migrations()?;
    let _ = apply_diff_from_store_with_conn(&conn, diff_id, repo_path)?;
    Ok(())
}

fn apply_diff_from_store_with_conn(
    conn: &Connection,
    diff_id: &str,
    repo_path: &Path,
) -> Result<AppliedDiffMeta, Box<dyn std::error::Error>> {
    let row = load_diff_row(conn, diff_id)?;
    let token = load_token(diff_id)?;

    let uuid = Uuid::parse_str(diff_id)?;
    let diff = DiffProposal {
        id: uuid,
        project_id: row.project_id.clone(),
        diff_text: row.patch_unified,
        file_path: row.first_file,
        created_at: row.created_at,
    };

    apply_diff(
        conn,
        &token,
        "coder-primary",
        &row.task_id,
        repo_path,
        &row.project_id,
        &diff,
    )?;

    remove_token(diff_id)?;

    // Write decision to memory so the agent can recall it in future sessions.
    let decision = Decision {
        id: format!("decision-{diff_id}"),
        project_id: "default".to_string(),
        agent_id: "coder-primary".to_string(),
        task_id: row.task_id.clone(),
        summary: format!("Applied patch to {}", diff.file_path),
        rationale: Some(format!("diff_id={diff_id}")),
        context_refs: None,
        outcome: "approved".to_string(),
    };
    // Best-effort: log error but don't fail the apply.
    if let Err(e) = write_decision(conn, &decision) {
        eprintln!("warning: failed to write decision: {e}");
    }

    let memory = Memory {
        id: format!("mem-apply-{diff_id}"),
        project_id: "default".to_string(),
        agent_id: "coder-primary".to_string(),
        scope: "project".to_string(),
        content: format!(
            "Applied change to {file}: {summary}. diff_id={diff_id}",
            file = diff.file_path,
            summary = decision.summary,
        ),
        tags: Some("apply,decision".to_string()),
        importance: 0.8,
    };
    if let Err(e) = insert_memory(conn, &row.task_id, &memory) {
        eprintln!("warning: failed to write apply memory: {e}");
    }

    Ok(AppliedDiffMeta {
        task_id: row.task_id,
    })
}
struct AppliedDiffMeta {
    task_id: String,
}

struct StoredDiff {
    task_id: String,
    project_id: String,
    patch_unified: String,
    first_file: String,
    created_at: i64,
}

fn run_post_apply_verify(
    conn: &Connection,
    repo_path: &Path,
    task_id: &str,
    diff_id: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    let verify_row: Option<(Option<String>, i64)> = conn
        .query_row(
            "SELECT test_command, verify_timeout_secs
             FROM agent_state
             WHERE agent_id = 'coder-primary' AND project_id = 'default'
             LIMIT 1",
            [],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .optional()?;

    let (test_command_opt, verify_timeout_secs) = match verify_row {
        Some(v) => v,
        None => {
            println!(
                "Error: no test_command configured. Set one with:\n scos profile use --test-command \"<cmd>\""
            );
            std::process::exit(1);
        }
    };

    let test_command = match test_command_opt {
        Some(cmd) if !cmd.trim().is_empty() => cmd,
        _ => {
            println!(
                "Error: no test_command configured. Set one with:\n scos profile use --test-command \"<cmd>\""
            );
            std::process::exit(1);
        }
    };

    let timeout_secs = u64::try_from(verify_timeout_secs).unwrap_or(60);
    let outcome = match run_verify(repo_path, &test_command, timeout_secs) {
        Ok(value) => value,
        Err(err) => {
            eprintln!("{err}");
            std::process::exit(1);
        }
    };

    if outcome.exit_code == 0 && !outcome.timed_out {
        let output_json = json!({
            "exit_code": 0,
            "stdout": outcome.stdout_truncated,
            "stderr": outcome.stderr_truncated,
            "elapsed_ms": outcome.elapsed_ms,
            "timed_out": false,
        })
        .to_string();
        record_verify_event_best_effort(
            conn,
            task_id,
            diff_id,
            EventType::TestVerifyPassed,
            output_json,
        );
        println!("✓ Tests passed ({}ms)", outcome.elapsed_ms);
        return Ok(());
    }

    let output_json = json!({
        "exit_code": outcome.exit_code,
        "stdout": outcome.stdout_truncated,
        "stderr": outcome.stderr_truncated,
        "elapsed_ms": outcome.elapsed_ms,
        "timed_out": outcome.timed_out,
    })
    .to_string();
    record_verify_event_best_effort(
        conn,
        task_id,
        diff_id,
        EventType::ApplyUnverified,
        output_json,
    );

    println!(
        "⚠ Tests did not pass (exit={}, timed_out={}). Files preserved on disk.",
        outcome.exit_code, outcome.timed_out
    );
    std::process::exit(2);
}

fn record_verify_event_best_effort(
    conn: &Connection,
    task_id: &str,
    diff_id: &str,
    event_type: EventType,
    output_json: String,
) {
    let created_at = match now_unix() {
        Ok(v) => v,
        Err(_) => {
            eprintln!("warning: failed to record verify event: invalid system time");
            return;
        }
    };

    let payload = format!("verify:{}:{}:{}", event_type.as_str(), task_id, diff_id);
    let event = ToolEvent {
        id: content_id(payload.as_bytes()),
        task_id: task_id.to_string(),
        agent_id: "coder-primary".to_string(),
        event_type,
        tool_name: Some("run_verify".to_string()),
        inputs_hash: None,
        inputs_json: None,
        output_hash: None,
        output_json: Some(output_json),
        approval_token_id: None,
        diff_id: Some(diff_id.to_string()),
        profile_name: None,
        created_at,
    };

    if let Err(err) = append_event(conn, &event) {
        eprintln!("warning: failed to record verify event: {err}");
    }
}

fn open_db_with_migrations() -> Result<Connection, Box<dyn std::error::Error>> {
    let db_path = std::env::current_dir()?.join("skycode.db");
    let conn = Connection::open(db_path)?;

    let migrations_dir = std::env::current_dir()?.join("memory").join("migrations");
    if migrations_dir.exists() {
        let _ = run_migrations(&conn, &migrations_dir)?;
    }

    Ok(conn)
}

fn now_unix() -> Result<i64, Box<dyn std::error::Error>> {
    let secs = SystemTime::now().duration_since(UNIX_EPOCH)?.as_secs();
    Ok(i64::try_from(secs)?)
}

fn load_diff_row(
    conn: &Connection,
    diff_id: &str,
) -> Result<StoredDiff, Box<dyn std::error::Error>> {
    let mut stmt = conn.prepare(
        "SELECT task_id, project_id, patch_unified, affected_files_json, created_at
         FROM diff_proposals WHERE id = ?1",
    )?;

    let row = stmt.query_row(params![diff_id], |r| {
        Ok((
            r.get::<_, String>(0)?,
            r.get::<_, String>(1)?,
            r.get::<_, String>(2)?,
            r.get::<_, String>(3)?,
            r.get::<_, i64>(4)?,
        ))
    })?;

    let files_opt: Option<Vec<String>> = serde_json::from_str(&row.3).unwrap_or(None);
    let first_file = files_opt
        .and_then(|v| v.first().cloned())
        .unwrap_or_else(|| "README.md".to_string());

    Ok(StoredDiff {
        task_id: row.0,
        project_id: row.1,
        patch_unified: row.2,
        first_file,
        created_at: row.4,
    })
}
