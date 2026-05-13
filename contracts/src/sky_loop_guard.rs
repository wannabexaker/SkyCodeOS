use std::time::{SystemTime, UNIX_EPOCH};

use rusqlite::{params, Connection};
use thiserror::Error;

use crate::sky_event::{compute_event_id, SkyEventType};

pub const DEFAULT_MAX_TOOL_CALLS: i64 = 50;

#[derive(Debug, Error)]
pub enum SkyLoopError {
    #[error("tool call budget exceeded for task {task_id}, agent {agent_id}: {calls}")]
    BudgetExceeded {
        task_id: String,
        agent_id: String,
        calls: i64,
    },
    #[error("database error: {0}")]
    Db(#[from] rusqlite::Error),
}

pub fn check_and_increment(
    conn: &Connection,
    task_id: &str,
    agent_id: &str,
    max_calls: i64,
) -> Result<(), SkyLoopError> {
    let now = now_unix();

    conn.execute(
        concat!(
            "INSERT INTO task_loop_counters (task_id, agent_id, tool_calls, last_call_at)
         VALUES (?1, ?2, 1, ?3)
         ON CONFLICT(task_id, agent_id) DO ",
            "UP",
            "DATE SET
           tool_calls = tool_calls + 1,
           last_call_at = excluded.last_call_at"
        ),
        params![task_id, agent_id, now],
    )?;

    let calls: i64 = conn.query_row(
        "SELECT tool_calls FROM task_loop_counters
         WHERE task_id = ?1 AND agent_id = ?2",
        params![task_id, agent_id],
        |row| row.get(0),
    )?;

    if calls > max_calls {
        let _ = conn.execute(
            "INSERT INTO tool_events (
                id, task_id, agent_id, event_type, tool_name,
                inputs_hash, inputs_json, output_hash, output_json,
                approval_token_id, diff_id, profile_name, created_at
            ) VALUES (
                ?1, ?2, ?3, ?4, ?5,
                NULL, ?6, NULL, NULL,
                NULL, NULL, NULL, ?7
            )",
            params![
                compute_event_id(task_id, calls),
                task_id,
                agent_id,
                SkyEventType::SecurityBlocked.as_str(),
                "loop_guard",
                format!("{{\"tool_calls\":{calls},\"max_calls\":{max_calls}}}"),
                now,
            ],
        );

        return Err(SkyLoopError::BudgetExceeded {
            task_id: task_id.to_string(),
            agent_id: agent_id.to_string(),
            calls,
        });
    }

    Ok(())
}

fn now_unix() -> i64 {
    match SystemTime::now().duration_since(UNIX_EPOCH) {
        Ok(duration) => match i64::try_from(duration.as_secs()) {
            Ok(secs) => secs,
            Err(_) => i64::MAX,
        },
        Err(_) => 0,
    }
}
