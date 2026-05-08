use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

use ring::rand::SystemRandom;
use ring::signature::Ed25519KeyPair;
use rusqlite::Connection;
use skycode_runtime::approval::token::{public_key_bytes, ApprovalToken};
use skycode_runtime::approval::validator::{register_signing_key, validate_token};
use skycode_runtime::db::events::{append_event, content_id, EventType, ToolEvent};
use skycode_runtime::db::migrations::run_migrations;
use skycode_runtime::tools::apply::apply_diff;
use skycode_runtime::tools::diff::create_diff;
use skycode_runtime::tools::rollback::rollback;

#[test]
fn phase1_gate_50_edit_cycles_zero_unapproved_writes() -> Result<(), Box<dyn std::error::Error>> {
    let conn = Connection::open_in_memory()?;
    let precheck_conn = Connection::open_in_memory()?;

    run_migrations_or_bootstrap(&conn)?;
    run_migrations_or_bootstrap(&precheck_conn)?;

    let agent_id = "coder-primary";

    let rng = SystemRandom::new();
    let pkcs8 =
        Ed25519KeyPair::generate_pkcs8(&rng).map_err(|_| "failed to generate Ed25519 key pair")?;
    let key_pair = Ed25519KeyPair::from_pkcs8(pkcs8.as_ref())
        .map_err(|_| "failed to parse Ed25519 key pair")?;
    // Compute hex once; register on both connections before the cycle loop.
    let pub_key_hex: String = public_key_bytes(&key_pair)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect();
    register_signing_key(&conn, agent_id, &pub_key_hex, unix_now()?)?;
    register_signing_key(&precheck_conn, agent_id, &pub_key_hex, unix_now()?)?;

    let repo_root = create_temp_git_repo()?;
    let relative_file = Path::new("phase1_gate.txt");
    let absolute_file = repo_root.join(relative_file);
    let baseline = "anchor-top\nbaseline-phase1\nanchor-bottom\n";
    fs::write(&absolute_file, baseline)?;

    for cycle in 0..50 {
        fs::write(&absolute_file, baseline)?;
        let current = fs::read_to_string(&absolute_file)?;
        assert_eq!(current, baseline);

        let before = baseline.to_string();
        let after_middle = format!("cycle-{cycle}-approved");
        let after = format!("anchor-top\n{}\nanchor-bottom\n", after_middle);

        let mut diff = create_diff(relative_file, &before, &after)?;
        diff.diff_text = format!(
            "diff --git a/{path} b/{path}\n--- a/{path}\n+++ b/{path}\n@@ -1,3 +1,3 @@\n anchor-top\n-baseline-phase1\n+{after_middle}\n anchor-bottom\n",
            path = relative_file.display(),
            after_middle = after_middle,
        );
        let token = ApprovalToken::create_signed(
            diff.id.to_string(),
            agent_id,
            format!("nonce-{cycle}"),
            &key_pair,
        )?;

        // Preflight validation step required by the gate test.
        // apply_diff() validates again on the main connection.
        validate_token(
            &precheck_conn,
            &token,
            &diff.id.to_string(),
            agent_id,
            &format!("task-precheck-{cycle}"),
        )?;

        apply_diff(
            &conn,
            &token,
            agent_id,
            &format!("task-apply-{cycle}"),
            &repo_root,
            &diff,
        )?;

        let payload = format!(
            "{}:{}:{}:{}",
            cycle,
            token.id,
            diff.id,
            EventType::DiffApplied.as_str()
        );
        let event = ToolEvent {
            id: content_id(payload.as_bytes()),
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
        };
        append_event(&conn, &event)?;

        rollback(&repo_root, "HEAD")?;
        let rolled_back = fs::read_to_string(&absolute_file)?;
        assert_eq!(rolled_back, baseline);
    }

    let total_events: i64 =
        conn.query_row("SELECT COUNT(*) FROM tool_events", [], |row| row.get(0))?;
    assert_eq!(total_events, 50);

    let diff_applied_events: i64 = conn.query_row(
        "SELECT COUNT(*) FROM tool_events WHERE event_type = 'diff_applied'",
        [],
        |row| row.get(0),
    )?;
    assert_eq!(diff_applied_events, 50);

    let unapproved_writes: i64 = conn.query_row(
        "SELECT COUNT(*) FROM tool_events WHERE event_type = 'diff_applied' AND approval_token_id IS NULL",
        [],
        |row| row.get(0),
    )?;
    assert_eq!(unapproved_writes, 0);

    Ok(())
}

fn run_migrations_or_bootstrap(conn: &Connection) -> Result<(), Box<dyn std::error::Error>> {
    let migrations_dir = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("memory")
        .join("migrations");

    if migrations_dir.exists() {
        run_migrations(conn, &migrations_dir)?;
    }

    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS approval_tokens_used (
            tid     TEXT PRIMARY KEY,
            diff_id TEXT NOT NULL,
            task_id TEXT NOT NULL,
            used_at INTEGER NOT NULL
        ) STRICT;

        CREATE TABLE IF NOT EXISTS tool_events (
            id                TEXT PRIMARY KEY,
            task_id           TEXT NOT NULL,
            agent_id          TEXT NOT NULL,
            event_type        TEXT NOT NULL,
            tool_name         TEXT,
            inputs_hash       TEXT,
            inputs_json       TEXT,
            output_hash       TEXT,
            output_json       TEXT,
            approval_token_id TEXT,
            diff_id           TEXT,
            profile_name      TEXT,
            created_at        INTEGER NOT NULL
        ) STRICT;

        CREATE TABLE IF NOT EXISTS signing_keys (
            agent_id       TEXT PRIMARY KEY,
            public_key_hex TEXT NOT NULL,
            registered_at  INTEGER NOT NULL
        ) STRICT;",
    )?;

    Ok(())
}

fn create_temp_git_repo() -> Result<PathBuf, Box<dyn std::error::Error>> {
    let repo = std::env::temp_dir().join(format!(
        "skycode-phase1-gate-{}",
        SystemTime::now().duration_since(UNIX_EPOCH)?.as_nanos()
    ));

    fs::create_dir_all(&repo)?;

    run_git(&repo, &["init"])?;
    run_git(&repo, &["config", "core.autocrlf", "false"])?;
    run_git(&repo, &["config", "core.eol", "lf"])?;
    run_git(
        &repo,
        &["config", "user.email", "phase1-gate@skycode.local"],
    )?;
    run_git(&repo, &["config", "user.name", "phase1-gate"])?;

    let file_path = repo.join("phase1_gate.txt");
    fs::write(&file_path, "anchor-top\nbaseline-phase1\nanchor-bottom\n")?;

    run_git(&repo, &["add", "."])?;
    run_git(&repo, &["commit", "-m", "baseline"])?;

    Ok(repo)
}

fn run_git(repo: &Path, args: &[&str]) -> Result<(), Box<dyn std::error::Error>> {
    let output = Command::new("git")
        .arg("-C")
        .arg(repo)
        .args(args)
        .output()?;
    if !output.status.success() {
        return Err(format!(
            "git command failed: git -C {} {}\n{}",
            repo.display(),
            args.join(" "),
            String::from_utf8_lossy(&output.stderr)
        )
        .into());
    }
    Ok(())
}

fn unix_now() -> Result<i64, Box<dyn std::error::Error>> {
    let secs = SystemTime::now().duration_since(UNIX_EPOCH)?.as_secs();
    Ok(i64::try_from(secs)?)
}
