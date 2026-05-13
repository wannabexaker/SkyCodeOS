use std::path::{Path, PathBuf};

use rusqlite::{params, Connection};
use serde_json::json;
use skycode_contracts::sky_redact::redact_payload;
use skycode_contracts::sky_task::SkyTaskRequest;
use skycode_runtime::db::migrations::run_migrations;
use tempfile::TempDir;

#[test]
fn phase9_redact_flat() {
    let mut payload = json!({
        "api_key": "s3cr3t",
        "result": "ok",
    });

    redact_payload(&mut payload);

    assert_eq!(payload["api_key"], "[REDACTED]");
    assert_eq!(payload["result"], "ok");
}

#[test]
fn phase9_redact_nested() {
    let mut payload = json!({
        "auth": {
            "token": "abc",
            "user": "x",
        }
    });

    redact_payload(&mut payload);

    assert_eq!(payload["auth"]["token"], "[REDACTED]");
    assert_eq!(payload["auth"]["user"], "x");
}

#[test]
fn phase9_redact_no_mutation() {
    let mut payload = json!({
        "count": 42,
        "name": "alice",
    });
    let original = payload.clone();

    redact_payload(&mut payload);

    assert_eq!(payload, original);
}

#[test]
fn phase9_sky_task_roundtrip() -> Result<(), Box<dyn std::error::Error>> {
    let request = SkyTaskRequest {
        agent_id: "coder-primary".to_string(),
        goal: "Implement bridge".to_string(),
        mode: Some("diff".to_string()),
        quest_id: Some("quest-1".to_string()),
        guild_id: Some("guild-1".to_string()),
        external_ref: Some(json!({ "source": "skairpg", "id": 7 })),
    };

    let encoded = serde_json::to_string(&request)?;
    let decoded: SkyTaskRequest = serde_json::from_str(&encoded)?;

    assert_eq!(decoded.agent_id, "coder-primary");
    assert_eq!(decoded.goal, "Implement bridge");
    assert_eq!(decoded.mode.as_deref(), Some("diff"));
    assert_eq!(decoded.quest_id.as_deref(), Some("quest-1"));
    assert_eq!(decoded.guild_id.as_deref(), Some("guild-1"));
    assert_eq!(
        decoded.external_ref,
        Some(json!({ "source": "skairpg", "id": 7 }))
    );

    Ok(())
}

#[test]
fn phase9_task_insert() -> Result<(), Box<dyn std::error::Error>> {
    let temp = TempDir::new()?;
    let conn = Connection::open(temp.path().join("phase9.db"))?;
    run_migrations(&conn, &migrations_dir())?;

    conn.execute(
        "INSERT INTO submitted_tasks (
            id, agent_id, goal, mode, status, quest_id, guild_id, external_ref, created_at
         ) VALUES (
            ?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9
         )",
        params![
            "task-1",
            "coder-primary",
            "Build the bridge",
            "diff",
            "accepted",
            "quest-1",
            "guild-1",
            r#"{"source":"test"}"#,
            123_i64,
        ],
    )?;

    let row = conn.query_row(
        "SELECT id, agent_id, goal, mode, status, quest_id, guild_id, external_ref, created_at
         FROM submitted_tasks
         WHERE id = ?1",
        ["task-1"],
        |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, String>(3)?,
                row.get::<_, String>(4)?,
                row.get::<_, Option<String>>(5)?,
                row.get::<_, Option<String>>(6)?,
                row.get::<_, Option<String>>(7)?,
                row.get::<_, i64>(8)?,
            ))
        },
    )?;

    assert_eq!(row.0, "task-1");
    assert_eq!(row.1, "coder-primary");
    assert_eq!(row.2, "Build the bridge");
    assert_eq!(row.3, "diff");
    assert_eq!(row.4, "accepted");
    assert_eq!(row.5.as_deref(), Some("quest-1"));
    assert_eq!(row.6.as_deref(), Some("guild-1"));
    assert_eq!(row.7.as_deref(), Some(r#"{"source":"test"}"#));
    assert_eq!(row.8, 123);

    Ok(())
}

fn migrations_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("memory")
        .join("migrations")
}
