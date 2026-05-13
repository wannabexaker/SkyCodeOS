//! Phase 6 Pillar 1 — Append-only trigger verification.
//!
//! The schema enforces that `tool_events`, `approval_tokens_used`,
//! `applied_changes`, `diff_proposals`, and `diff_set_members` are
//! insert-only.  Any UPDATE or DELETE must be aborted by a BEFORE trigger.
//!
//! These tests exercise the triggers directly via raw SQL so that no
//! application-layer guard can mask a missing trigger.

use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use rusqlite::{params, Connection};
use skycode_runtime::db::migrations::run_migrations;

fn open_migrated_mem_db() -> Result<Connection, Box<dyn std::error::Error>> {
    let conn = Connection::open_in_memory()?;
    run_migrations(&conn, &migrations_dir())?;
    Ok(conn)
}

fn migrations_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("memory")
        .join("migrations")
}

fn now() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs() as i64
}

// ─── tool_events ─────────────────────────────────────────────────────────────

fn insert_tool_event(conn: &Connection, id: &str) -> rusqlite::Result<()> {
    conn.execute(
        "INSERT INTO tool_events
            (id, task_id, agent_id, event_type, created_at)
         VALUES (?1, 'task-1', 'coder-primary', 'model_invoked', ?2)",
        params![id, now()],
    )?;
    Ok(())
}

#[test]
fn phase6_append_only_tool_events_update_blocked() -> Result<(), Box<dyn std::error::Error>> {
    let conn = open_migrated_mem_db()?;
    insert_tool_event(&conn, "ev-1")?;

    let err = conn
        .execute(
            "UPDATE tool_events SET task_id = 'tampered' WHERE id = 'ev-1'",
            [],
        )
        .expect_err("UPDATE on tool_events must be blocked");

    let msg = err.to_string();
    assert!(
        msg.contains("append-only") || msg.contains("ABORT"),
        "expected append-only trigger error, got: {msg}"
    );
    Ok(())
}

#[test]
fn phase6_append_only_tool_events_delete_blocked() -> Result<(), Box<dyn std::error::Error>> {
    let conn = open_migrated_mem_db()?;
    insert_tool_event(&conn, "ev-2")?;

    let err = conn
        .execute("DELETE FROM tool_events WHERE id = 'ev-2'", [])
        .expect_err("DELETE on tool_events must be blocked");

    let msg = err.to_string();
    assert!(
        msg.contains("append-only") || msg.contains("ABORT"),
        "expected append-only trigger error, got: {msg}"
    );
    Ok(())
}

// ─── approval_tokens_used ────────────────────────────────────────────────────

fn insert_token_used(conn: &Connection, tid: &str) -> rusqlite::Result<()> {
    conn.execute(
        "INSERT INTO approval_tokens_used (tid, diff_id, task_id, used_at)
         VALUES (?1, 'diff-x', 'task-1', ?2)",
        params![tid, now()],
    )?;
    Ok(())
}

#[test]
fn phase6_append_only_tokens_update_blocked() -> Result<(), Box<dyn std::error::Error>> {
    let conn = open_migrated_mem_db()?;
    insert_token_used(&conn, "tok-1")?;

    let err = conn
        .execute(
            "UPDATE approval_tokens_used SET diff_id = 'tampered' WHERE tid = 'tok-1'",
            [],
        )
        .expect_err("UPDATE on approval_tokens_used must be blocked");

    assert!(
        err.to_string().contains("append-only") || err.to_string().contains("ABORT"),
        "got: {err}"
    );
    Ok(())
}

#[test]
fn phase6_append_only_tokens_delete_blocked() -> Result<(), Box<dyn std::error::Error>> {
    let conn = open_migrated_mem_db()?;
    insert_token_used(&conn, "tok-2")?;

    let err = conn
        .execute("DELETE FROM approval_tokens_used WHERE tid = 'tok-2'", [])
        .expect_err("DELETE on approval_tokens_used must be blocked");

    assert!(
        err.to_string().contains("append-only") || err.to_string().contains("ABORT"),
        "got: {err}"
    );
    Ok(())
}

// ─── diff_set_members ────────────────────────────────────────────────────────

fn insert_diff_set_and_member(conn: &Connection) -> rusqlite::Result<()> {
    conn.execute(
        "INSERT INTO diff_sets (set_id, task_id, agent_id, project_id, created_at)
         VALUES ('s1', 'task-1', 'coder-primary', 'default', ?1)",
        params![now()],
    )?;
    // Insert member BEFORE the set exists check fires (set is already committed above).
    // The trigger allows inserts when set_id is NOT in diff_sets; since we just inserted
    // the set, the trigger WILL fire and block — so we use a raw INSERT here which is
    // the path that SHOULD be allowed during initial creation.
    // To insert members for testing the update/delete triggers we use the application
    // function, which inserts members BEFORE the set record (bypassing the trigger).
    // For simplicity here we insert a member directly while the set is NOT yet committed
    // by opening a transaction that inserts member first.
    conn.execute(
        "INSERT INTO diff_set_members (set_id, diff_id, ord) VALUES ('s2', 'd1', 1)",
        [],
    )
    .ok(); // May or may not succeed depending on set existence; covered separately.
    Ok(())
}

#[test]
fn phase6_append_only_diff_set_members_update_blocked() -> Result<(), Box<dyn std::error::Error>> {
    let conn = open_migrated_mem_db()?;
    // Use the application function to create set + member atomically (members first).
    skycode_runtime::db::create_diff_set(
        &conn,
        &skycode_runtime::db::DiffSetRecord {
            set_id: "s-upd".to_string(),
            task_id: "task-1".to_string(),
            agent_id: "coder-primary".to_string(),
            project_id: "default".to_string(),
            created_at: now(),
        },
        &[("d1".to_string(), 1)],
    )?;

    let err = conn
        .execute(
            "UPDATE diff_set_members SET ord = 99 WHERE set_id = 's-upd'",
            [],
        )
        .expect_err("UPDATE on diff_set_members must be blocked");

    assert!(
        err.to_string().contains("append-only") || err.to_string().contains("ABORT"),
        "got: {err}"
    );
    Ok(())
}

#[test]
fn phase6_append_only_diff_set_members_delete_blocked() -> Result<(), Box<dyn std::error::Error>> {
    let conn = open_migrated_mem_db()?;
    skycode_runtime::db::create_diff_set(
        &conn,
        &skycode_runtime::db::DiffSetRecord {
            set_id: "s-del".to_string(),
            task_id: "task-1".to_string(),
            agent_id: "coder-primary".to_string(),
            project_id: "default".to_string(),
            created_at: now(),
        },
        &[("d1".to_string(), 1)],
    )?;

    let err = conn
        .execute("DELETE FROM diff_set_members WHERE set_id = 's-del'", [])
        .expect_err("DELETE on diff_set_members must be blocked");

    assert!(
        err.to_string().contains("append-only") || err.to_string().contains("ABORT"),
        "got: {err}"
    );
    Ok(())
}
