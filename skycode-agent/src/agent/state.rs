use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use thiserror::Error;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentState {
    pub agent_id: String,
    pub project_id: String,
    pub current_task: Option<String>,
    pub status: String,
    pub session_id: Option<String>,
    pub updated_at: i64,
}

#[derive(Debug, Error)]
pub enum AgentStateError {
    #[error("database error: {0}")]
    Database(#[from] rusqlite::Error),
    #[error("serialization error: {0}")]
    Serialize(#[from] serde_json::Error),
}

pub fn load_state(
    conn: &Connection,
    agent_id: &str,
    project_id: &str,
) -> Result<Option<AgentState>, AgentStateError> {
    let mut stmt = conn.prepare(
        "SELECT state_json, session_id, updated_at
         FROM agent_state
         WHERE agent_id = ?1 AND project_id = ?2",
    )?;

    let row: Option<(String, Option<String>, i64)> = stmt
        .query_row(params![agent_id, project_id], |r| {
            Ok((r.get(0)?, r.get(1)?, r.get(2)?))
        })
        .optional()?;

    let Some((state_json, session_id, updated_at)) = row else {
        return Ok(None);
    };

    let mut parsed: AgentState = serde_json::from_str(&state_json)?;
    parsed.session_id = session_id;
    parsed.updated_at = updated_at;
    Ok(Some(parsed))
}

pub fn save_state(conn: &Connection, state: &AgentState) -> Result<(), AgentStateError> {
    let state_json = serde_json::to_string(state)?;

    let mut stmt = conn.prepare(
        "INSERT INTO agent_state (agent_id, project_id, state_json, session_id, updated_at)
         VALUES (?1, ?2, ?3, ?4, ?5)
         ON CONFLICT(agent_id, project_id) DO UPDATE SET
            state_json = excluded.state_json,
            session_id = excluded.session_id,
            updated_at = excluded.updated_at",
    )?;

    stmt.execute(params![
        state.agent_id,
        state.project_id,
        state_json,
        state.session_id,
        state.updated_at,
    ])?;

    Ok(())
}
