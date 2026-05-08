use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use rusqlite::{params, Connection};
use skycode_runtime::agent::{load_coder_primary_identity, load_state, save_state, AgentState};
use skycode_runtime::db::events::{append_event, content_id, EventType, ToolEvent};
use skycode_runtime::db::migrations::run_migrations;
use skycode_runtime::memory::search_memories;
use skycode_runtime::memory::store::{insert_memory, Decision, Memory};
use skycode_runtime::orchestrator::write_decision;

#[test]
fn test_agent_state_persists_across_restart() -> Result<(), Box<dyn std::error::Error>> {
    let db_path = temp_db_path("phase4-agent-state");
    if db_path.exists() {
        fs::remove_file(&db_path)?;
    }

    {
        let conn = Connection::open(&db_path)?;
        apply_phase_schema(&conn)?;

        let state = AgentState {
            agent_id: "coder-primary".to_string(),
            project_id: "test".to_string(),
            current_task: Some("refactor auth".to_string()),
            status: "complete".to_string(),
            session_id: Some("s1".to_string()),
            updated_at: unix_now()?,
        };

        save_state(&conn, &state)?;
    }

    {
        let conn = Connection::open(&db_path)?;
        let loaded =
            load_state(&conn, "coder-primary", "test")?.ok_or("expected state row after reopen")?;

        assert_eq!(loaded.current_task.as_deref(), Some("refactor auth"));
        assert_eq!(loaded.session_id.as_deref(), Some("s1"));
    }

    let _ = fs::remove_file(&db_path);

    Ok(())
}

#[test]
fn test_decision_written_and_queryable() -> Result<(), Box<dyn std::error::Error>> {
    let conn = Connection::open_in_memory()?;
    apply_phase_schema(&conn)?;

    let decision = Decision {
        id: "dec-1".to_string(),
        project_id: "proj-a".to_string(),
        agent_id: "coder-primary".to_string(),
        task_id: "task-42".to_string(),
        summary: "approved auth refactor".to_string(),
        rationale: Some("all tests green".to_string()),
        context_refs: Some("memory:m1,graph:symbol:s2".to_string()),
        outcome: "approved".to_string(),
    };

    write_decision(&conn, &decision)?;

    let mut stmt = conn.prepare(
        "SELECT summary, rationale, created_at
         FROM decisions
         WHERE agent_id = ?1 AND task_id = ?2
         LIMIT 1",
    )?;

    let row = stmt.query_row(params!["coder-primary", "task-42"], |r| {
        Ok((
            r.get::<_, String>(0)?,
            r.get::<_, Option<String>>(1)?,
            r.get::<_, i64>(2)?,
        ))
    })?;

    assert_eq!(row.0, "approved auth refactor");
    assert_eq!(row.1.as_deref(), Some("all tests green"));
    assert!(row.2 > 0, "created_at must be non-zero");

    Ok(())
}

#[test]
fn test_exactly_one_agent_assertion() -> Result<(), Box<dyn std::error::Error>> {
    let agents_root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("agents");

    let identity = load_coder_primary_identity(&agents_root)?;

    assert_eq!(identity.id, "coder-primary");
    assert!(identity
        .must_never
        .iter()
        .any(|v| v == "write_without_approval"));
    assert!(identity
        .approval_required_for
        .iter()
        .any(|v| v == "file_write"));

    Ok(())
}

#[test]
fn test_model_invoked_event_carries_profile_name() -> Result<(), Box<dyn std::error::Error>> {
    let conn = Connection::open_in_memory()?;
    apply_phase_schema(&conn)?;

    let payload = b"model-invoked:task-77:precise";
    let event = ToolEvent {
        id: content_id(payload),
        task_id: "task-77".to_string(),
        agent_id: "coder-primary".to_string(),
        event_type: EventType::ModelInvoked,
        tool_name: Some("llama-cli".to_string()),
        inputs_hash: None,
        inputs_json: None,
        output_hash: None,
        output_json: None,
        approval_token_id: None,
        diff_id: None,
        profile_name: Some("precise".to_string()),
        created_at: unix_now()?,
    };

    append_event(&conn, &event)?;

    let mut stmt = conn.prepare(
        "SELECT profile_name, approval_token_id
         FROM tool_events
         WHERE task_id = ?1 AND event_type = 'model_invoked'
         LIMIT 1",
    )?;

    let row = stmt.query_row(params!["task-77"], |r| {
        Ok((
            r.get::<_, Option<String>>(0)?,
            r.get::<_, Option<String>>(1)?,
        ))
    })?;

    assert_eq!(row.0.as_deref(), Some("precise"));
    assert!(
        row.1.is_none(),
        "model_invoked should not require approval token"
    );

    Ok(())
}

#[test]
fn test_decision_recall_across_connections() -> Result<(), Box<dyn std::error::Error>> {
    let tmpdir = tempfile::tempdir()?;
    let db_path = tmpdir.path().join("test.db");
    let migrations_dir = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("memory")
        .join("migrations");

    {
        let conn = Connection::open(&db_path)?;
        run_migrations(&conn, &migrations_dir)?;

        let memory = Memory {
            id: "mem-session-1-foo".to_string(),
            project_id: "default".to_string(),
            agent_id: "coder-primary".to_string(),
            scope: "project".to_string(),
            content: "Applied patch to src/lib.rs: add fn foo".to_string(),
            tags: Some("apply,decision".to_string()),
            importance: 0.8,
        };

        insert_memory(&conn, "task-session-1", &memory)?;
    }

    {
        let conn2 = Connection::open(&db_path)?;
        let results = search_memories(&conn2, "fn foo", "default", "coder-primary", "project", 5)?;

        assert!(!results.is_empty(), "expected recalled apply memory");
        assert!(
            results[0].content.contains("fn foo"),
            "top result should contain the recalled decision text"
        );
    }

    Ok(())
}

fn apply_phase_schema(conn: &Connection) -> Result<(), Box<dyn std::error::Error>> {
    let schema_path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("memory")
        .join("migrations")
        .join("001_initial.sql");

    let sql = fs::read_to_string(schema_path)?;
    conn.execute_batch(&sql)?;
    Ok(())
}

fn temp_db_path(prefix: &str) -> PathBuf {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    std::env::temp_dir().join(format!("{}-{}.db", prefix, nanos))
}

fn unix_now() -> Result<i64, Box<dyn std::error::Error>> {
    let secs = SystemTime::now().duration_since(UNIX_EPOCH)?.as_secs();
    Ok(i64::try_from(secs)?)
}
