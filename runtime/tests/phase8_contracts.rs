use rusqlite::Connection;
use serde_json::json;
use skycode_contracts::sky_capability::SkyCapabilityInfo;
use skycode_contracts::sky_cursor::SkyCursor;
use skycode_contracts::sky_event::{compute_event_id, SkyEvent, SkyEventType};
use skycode_contracts::sky_loop_guard::{
    check_and_increment, SkyLoopError, DEFAULT_MAX_TOOL_CALLS,
};

#[test]
fn phase8_sky_event_roundtrip() -> Result<(), Box<dyn std::error::Error>> {
    let event = SkyEvent::new(
        "runtime-test",
        42,
        "task-phase8",
        "coder-primary",
        "default",
        None,
        SkyEventType::DiffApplied,
        json!({ "diff_id": "diff-1" }),
        "1234567890",
    );

    let encoded = serde_json::to_string(&event)?;
    let decoded: SkyEvent = serde_json::from_str(&encoded)?;

    assert_eq!(decoded.cursor, 42);
    assert_eq!(decoded.task_id, "task-phase8");
    assert_eq!(decoded.agent_id, "coder-primary");
    assert_eq!(decoded.event_type, "diff.applied");
    assert_eq!(decoded.event_id, compute_event_id("task-phase8", 42));
    assert_eq!(decoded.payload["diff_id"], "diff-1");

    Ok(())
}

#[test]
fn phase8_event_id_deterministic() {
    let first = compute_event_id("task-phase8", 42);
    let second = compute_event_id("task-phase8", 42);
    let different = compute_event_id("task-phase8", 43);

    assert_eq!(first, second);
    assert_ne!(first, different);
}

#[test]
fn phase8_capability_defaults() {
    let info = SkyCapabilityInfo::default();

    assert!(info.local_first);
    assert!(info.requires_approval_token);
    assert_eq!(info.mcp_tool_names.len(), 8);
    assert_eq!(info.event_types.len(), 12);
    assert!(info.event_types.contains(&"security.blocked".to_string()));
}

#[test]
fn phase8_loop_guard_budget() -> Result<(), Box<dyn std::error::Error>> {
    let conn = loop_guard_conn()?;

    for _ in 0..DEFAULT_MAX_TOOL_CALLS {
        check_and_increment(&conn, "task-A", "coder-primary", DEFAULT_MAX_TOOL_CALLS)?;
    }

    let err = check_and_increment(&conn, "task-A", "coder-primary", DEFAULT_MAX_TOOL_CALLS)
        .expect_err("51st call must exceed budget");

    match err {
        SkyLoopError::BudgetExceeded {
            task_id,
            agent_id,
            calls,
        } => {
            assert_eq!(task_id, "task-A");
            assert_eq!(agent_id, "coder-primary");
            assert_eq!(calls, DEFAULT_MAX_TOOL_CALLS + 1);
        }
        SkyLoopError::Db(e) => return Err(Box::new(e)),
    }

    Ok(())
}

#[test]
fn phase8_loop_guard_new_task_resets() -> Result<(), Box<dyn std::error::Error>> {
    let conn = loop_guard_conn()?;

    for _ in 0..DEFAULT_MAX_TOOL_CALLS {
        check_and_increment(&conn, "task-A", "coder-primary", DEFAULT_MAX_TOOL_CALLS)?;
    }

    let _ = check_and_increment(&conn, "task-A", "coder-primary", DEFAULT_MAX_TOOL_CALLS)
        .expect_err("task-A should be over budget");

    check_and_increment(&conn, "task-B", "coder-primary", DEFAULT_MAX_TOOL_CALLS)?;

    let task_b_calls: i64 = conn.query_row(
        "SELECT tool_calls FROM task_loop_counters
         WHERE task_id = ?1 AND agent_id = ?2",
        ["task-B", "coder-primary"],
        |row| row.get(0),
    )?;
    assert_eq!(task_b_calls, 1);

    Ok(())
}

#[test]
fn phase8_cursor_default() {
    let cursor = SkyCursor::default();

    assert_eq!(cursor.after, 0);
    assert_eq!(cursor.limit, 100);
}

fn loop_guard_conn() -> Result<Connection, Box<dyn std::error::Error>> {
    let conn = Connection::open_in_memory()?;
    conn.execute_batch(include_str!("../../memory/migrations/0007_loop_guard.sql"))?;
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS tool_events (
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
        ) STRICT;",
    )?;
    Ok(conn)
}
