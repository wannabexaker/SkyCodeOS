use std::path::{Path, PathBuf};

use clap::Args;
use rusqlite::{params, Connection};
use uuid::Uuid;

use skycode_orchestrator::db::migrations::run_migrations;
use skycode_orchestrator::memory::store::{insert_memory, Decision, Memory};
use skycode_orchestrator::orchestrator::write_decision;
use skycode_orchestrator::tools::apply::apply_diff;
use skycode_orchestrator::tools::diff::DiffProposal;

use crate::commands::approve::{load_token, remove_token};

#[derive(Debug, Args)]
pub struct ApplyArgs {
    /// The UUID of the diff proposal to apply (requires prior approval).
    pub diff_id: String,

    /// Path to repository root where patch should be applied.
    #[arg(long, default_value = ".")]
    pub repo: PathBuf,
}

pub fn run(args: &ApplyArgs) -> Result<(), Box<dyn std::error::Error>> {
    apply_diff_from_store(&args.diff_id, &args.repo)?;

    println!("Applied diff: {}", args.diff_id);
    Ok(())
}

pub fn apply_diff_from_store(
    diff_id: &str,
    repo_path: &Path,
) -> Result<(), Box<dyn std::error::Error>> {
    let db_path = std::env::current_dir()?.join("skycode.db");
    let conn = Connection::open(db_path)?;

    let migrations_dir = std::env::current_dir()?.join("memory").join("migrations");
    if migrations_dir.exists() {
        let _ = run_migrations(&conn, &migrations_dir)?;
    }

    let row = load_diff_row(&conn, diff_id)?;
    let token = load_token(diff_id)?;

    let uuid = Uuid::parse_str(diff_id)?;
    let diff = DiffProposal {
        id: uuid,
        diff_text: row.patch_unified,
        file_path: row.first_file,
        created_at: row.created_at,
    };

    apply_diff(
        &conn,
        &token,
        "coder-primary",
        &row.task_id,
        repo_path,
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
    if let Err(e) = write_decision(&conn, &decision) {
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
    if let Err(e) = insert_memory(&conn, &row.task_id, &memory) {
        eprintln!("warning: failed to write apply memory: {e}");
    }

    Ok(())
}

struct StoredDiff {
    task_id: String,
    patch_unified: String,
    first_file: String,
    created_at: i64,
}

fn load_diff_row(
    conn: &Connection,
    diff_id: &str,
) -> Result<StoredDiff, Box<dyn std::error::Error>> {
    let mut stmt = conn.prepare(
        "SELECT task_id, patch_unified, affected_files_json, created_at
         FROM diff_proposals WHERE id = ?1",
    )?;

    let row = stmt.query_row(params![diff_id], |r| {
        Ok((
            r.get::<_, String>(0)?,
            r.get::<_, String>(1)?,
            r.get::<_, String>(2)?,
            r.get::<_, i64>(3)?,
        ))
    })?;

    let files_opt: Option<Vec<String>> = serde_json::from_str(&row.2).unwrap_or(None);
    let first_file = files_opt
        .and_then(|v| v.first().cloned())
        .unwrap_or_else(|| "README.md".to_string());

    Ok(StoredDiff {
        task_id: row.0,
        patch_unified: row.1,
        first_file,
        created_at: row.3,
    })
}
