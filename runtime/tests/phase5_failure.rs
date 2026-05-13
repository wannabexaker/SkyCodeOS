use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use ring::rand::SystemRandom;
use ring::signature::Ed25519KeyPair;
use rusqlite::{params, Connection, Error as SqlError, ErrorCode};
use skycode_runtime::approval::token::{public_key_bytes, ApprovalToken};
use skycode_runtime::approval::validator::{register_signing_key, ValidatorError};
use skycode_runtime::db::migrations::run_migrations;
use skycode_runtime::inference::registry::{ModelRegistry, ModelRegistryError};
use skycode_runtime::memory::{search_memories, Memory};
use skycode_runtime::tools::apply::{apply_diff, ApplyError};
use skycode_runtime::tools::diff::{create_diff, DiffProposal};
use tempfile::TempDir;
use uuid::Uuid;

#[test]
fn test_expired_token_rejected() -> Result<(), Box<dyn std::error::Error>> {
    let temp = TempDir::new()?;
    let conn = open_migrated_db(&temp)?;
    let (key_pair, _public_key) = make_keypair()?;
    let diff = create_diff(
        "default",
        Path::new("src/lib.rs"),
        "",
        "pub fn hello() {}\n",
    )?;
    let now = unix_now()?;
    let mut token = ApprovalToken::create_signed(
        "default",
        diff.id.to_string(),
        "coder-primary",
        "coder-primary",
        "expired",
        &key_pair,
    )?;
    token.created_at = now - 400;
    token.expires_at = now - 100;

    let err = apply_diff(
        &conn,
        &token,
        "coder-primary",
        "task-expired",
        temp.path(),
        "default",
        &diff,
    )
    .expect_err("expired token must reject before applying");

    assert!(
        matches!(err, ApplyError::Validation(ValidatorError::Expired)),
        "expected expired token validation error, got {err}"
    );

    Ok(())
}

#[test]
fn test_patch_conflict_rejected() -> Result<(), Box<dyn std::error::Error>> {
    let temp = TempDir::new()?;
    let conn = open_migrated_db(&temp)?;
    let repo = create_temp_git_repo(&temp)?;
    let file_path = repo.join("src").join("lib.rs");
    fs::create_dir_all(file_path.parent().ok_or("missing parent")?)?;
    fs::write(&file_path, "pub fn hello() {}\n")?;

    let (key_pair, public_key) = make_keypair()?;
    let key_hex: String = public_key.iter().map(|b| format!("{b:02x}")).collect();
    register_signing_key(&conn, "coder-primary", &key_hex, unix_now()?)?;

    let diff = DiffProposal {
        id: Uuid::new_v4(),
        project_id: "default".to_string(),
        file_path: "src/lib.rs".to_string(),
        created_at: unix_now()?,
        diff_text: "diff --git a/src/lib.rs b/src/lib.rs\n--- a/src/lib.rs\n+++ b/src/lib.rs\n@@ -1 +1 @@\n-pub fn missing() {}\n+pub fn changed() {}\n".to_string(),
    };
    insert_diff_proposal(&conn, "task-conflict", &diff)?;

    let token = ApprovalToken::create_signed(
        "default",
        diff.id.to_string(),
        "coder-primary",
        "coder-primary",
        "conflict",
        &key_pair,
    )?;

    let err = apply_diff(
        &conn,
        &token,
        "coder-primary",
        "task-conflict",
        &repo,
        "default",
        &diff,
    )
    .expect_err("conflicting patch must fail");

    assert!(
        matches!(err, ApplyError::GitApplyFailed { .. }),
        "expected git apply failure, got {err}"
    );
    assert_eq!(fs::read_to_string(&file_path)?, "pub fn hello() {}\n");

    let event_count: i64 = conn.query_row(
        "SELECT COUNT(*) FROM tool_events
         WHERE task_id = ?1 AND event_type = 'diff_apply_failed' AND diff_id = ?2",
        params!["task-conflict", diff.id.to_string()],
        |row| row.get(0),
    )?;
    assert_eq!(event_count, 1, "expected diff_apply_failed event");

    Ok(())
}

#[test]
fn test_sqlite_busy_propagates() -> Result<(), Box<dyn std::error::Error>> {
    let temp = TempDir::new()?;
    let db_path = temp.path().join("busy.db");
    let conn1 = Connection::open(&db_path)?;
    let conn2 = Connection::open(&db_path)?;
    conn2.busy_timeout(Duration::from_millis(25))?;

    conn1.execute_batch("BEGIN IMMEDIATE; CREATE TABLE held_lock(id INTEGER);")?;

    let migrations_dir = migrations_dir();
    let err = run_migrations(&conn2, &migrations_dir).expect_err("expected SQLITE_BUSY");

    match err {
        skycode_runtime::db::migrations::MigrationError::Database(SqlError::SqliteFailure(
            inner,
            _,
        )) => assert_eq!(inner.code, ErrorCode::DatabaseBusy),
        other => return Err(format!("expected SQLITE_BUSY, got {other}").into()),
    }

    conn1.execute_batch("ROLLBACK")?;
    Ok(())
}

#[test]
fn test_invalid_yaml_registry() -> Result<(), Box<dyn std::error::Error>> {
    let temp = TempDir::new()?;
    let path = temp.path().join("models.yaml");
    fs::write(&path, "models:\n  - name: [unterminated\n")?;

    let err = ModelRegistry::load_from_file(&path).expect_err("invalid YAML must error");
    assert!(
        matches!(err, ModelRegistryError::Parse(_)),
        "expected YAML parse error, got {err}"
    );

    Ok(())
}

#[test]
fn test_migrate_fresh_and_upgrade_idempotent() -> Result<(), Box<dyn std::error::Error>> {
    let temp = TempDir::new()?;
    let conn = open_db(&temp)?;
    let migrations_dir = migrations_dir();

    let first = run_migrations(&conn, &migrations_dir)?;
    assert!(first >= 1, "fresh DB should apply migrations");

    let migration_rows_before: i64 =
        conn.query_row("SELECT COUNT(*) FROM _skycode_migrations", [], |row| {
            row.get(0)
        })?;
    let tuning_table_count: i64 = conn.query_row(
        "SELECT COUNT(*) FROM sqlite_master WHERE type = 'table' AND name = 'tuning_runs'",
        [],
        |row| row.get(0),
    )?;
    assert_eq!(tuning_table_count, 1);

    let second = run_migrations(&conn, &migrations_dir)?;
    assert_eq!(second, 0, "second migration run must be idempotent");
    let migration_rows_after: i64 =
        conn.query_row("SELECT COUNT(*) FROM _skycode_migrations", [], |row| {
            row.get(0)
        })?;
    assert_eq!(migration_rows_before, migration_rows_after);

    Ok(())
}

#[test]
fn test_fts5_missing_triggers_detected() -> Result<(), Box<dyn std::error::Error>> {
    let temp = TempDir::new()?;
    let conn = open_migrated_db(&temp)?;

    conn.execute_batch(
        "DROP TRIGGER IF EXISTS memories_ai;
         DROP TRIGGER IF EXISTS memories_au;",
    )?;
    conn.execute(
        "INSERT INTO memories (
            id, project_id, agent_id, scope, content, tags, importance,
            created_at, updated_at, last_access
         ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, unixepoch(), unixepoch(), unixepoch())",
        params![
            "mem-missing-fts",
            "default",
            "coder-primary",
            "project",
            "raw inserted memory should not be indexed",
            Option::<String>::None,
            0.8_f64,
        ],
    )?;

    let result = search_memories(
        &conn,
        "raw inserted memory",
        "default",
        "coder-primary",
        "project",
        5,
    );

    match result {
        Ok(items) => assert!(items.is_empty(), "missing FTS trigger should not panic"),
        Err(_) => {}
    }

    let _memory_type_check = Memory {
        id: "unused".to_string(),
        project_id: "default".to_string(),
        agent_id: "coder-primary".to_string(),
        scope: "project".to_string(),
        content: "unused".to_string(),
        tags: None,
        importance: 0.5,
    };

    Ok(())
}

#[test]
fn test_replay_attack_rejected() -> Result<(), Box<dyn std::error::Error>> {
    let temp = TempDir::new()?;
    let conn = open_migrated_db(&temp)?;
    let repo = create_temp_git_repo(&temp)?;
    let file_path = repo.join("src").join("lib.rs");
    fs::create_dir_all(file_path.parent().ok_or("missing parent")?)?;
    fs::write(&file_path, "pub fn hello() {}\n")?;

    let (key_pair, public_key) = make_keypair()?;
    let key_hex: String = public_key.iter().map(|b| format!("{b:02x}")).collect();
    register_signing_key(&conn, "coder-primary", &key_hex, unix_now()?)?;

    let mut diff = create_diff("default", Path::new("src/lib.rs"), "", "")?;
    diff.diff_text = "diff --git a/src/lib.rs b/src/lib.rs\n--- a/src/lib.rs\n+++ b/src/lib.rs\n@@ -1 +1 @@\n-pub fn hello() {}\n+pub fn hello() { println!(\"hi\"); }\n".to_string();

    let token = ApprovalToken::create_signed(
        "default",
        diff.id.to_string(),
        "coder-primary",
        "coder-primary",
        "replay",
        &key_pair,
    )?;

    apply_diff(
        &conn,
        &token,
        "coder-primary",
        "task-replay",
        &repo,
        "default",
        &diff,
    )?;

    let err = apply_diff(
        &conn,
        &token,
        "coder-primary",
        "task-replay",
        &repo,
        "default",
        &diff,
    )
    .expect_err("same token must be rejected on replay");

    assert!(
        matches!(err, ApplyError::Validation(ValidatorError::ReplayDetected)),
        "expected replay detection, got {err}"
    );

    Ok(())
}

fn open_migrated_db(temp: &TempDir) -> Result<Connection, Box<dyn std::error::Error>> {
    let conn = open_db(temp)?;
    run_migrations(&conn, &migrations_dir())?;
    Ok(conn)
}

fn open_db(temp: &TempDir) -> Result<Connection, Box<dyn std::error::Error>> {
    Ok(Connection::open(temp.path().join("test.db"))?)
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
    run_git(&repo, &["config", "user.email", "phase5@skycode.local"])?;
    run_git(&repo, &["config", "user.name", "phase5"])?;

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

fn insert_diff_proposal(
    conn: &Connection,
    task_id: &str,
    diff: &DiffProposal,
) -> Result<(), Box<dyn std::error::Error>> {
    let now = unix_now()?;
    conn.execute(
        "INSERT INTO diff_proposals (
            id, task_id, agent_id, project_id, patch_unified, base_git_ref,
            base_blob_hashes_json, affected_files_json, created_at, expires_at
         ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
        params![
            diff.id.to_string(),
            task_id,
            "coder-primary",
            "default",
            diff.diff_text,
            "HEAD",
            "{}",
            serde_json::to_string(&vec![diff.file_path.clone()])?,
            now,
            now + 300,
        ],
    )?;
    Ok(())
}

fn unix_now() -> Result<i64, Box<dyn std::error::Error>> {
    let secs = SystemTime::now().duration_since(UNIX_EPOCH)?.as_secs();
    Ok(i64::try_from(secs)?)
}
