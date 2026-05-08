use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};
use std::time::{SystemTime, UNIX_EPOCH};
use thiserror::Error;

/// A memory record — scoped knowledge tied to a project and agent.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Memory {
    pub id: String,
    pub project_id: String,
    pub agent_id: String,
    pub scope: String, // 'project', 'agent', 'session', 'decision'
    pub content: String,
    pub tags: Option<String>,
    pub importance: f64,
}

/// A decision record — approval, rejection, or rollback outcome.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Decision {
    pub id: String,
    pub project_id: String,
    pub agent_id: String,
    pub task_id: String,
    pub summary: String,
    pub rationale: Option<String>,
    pub context_refs: Option<String>,
    pub outcome: String, // 'approved', 'rejected', 'rolled_back'
}

/// Agent session state — persisted across restarts.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentState {
    pub agent_id: String,
    pub project_id: String,
    pub state_json: String,
    pub session_id: Option<String>,
}

#[derive(Debug, Error)]
pub enum MemoryError {
    #[error("database error: {0}")]
    Database(#[from] rusqlite::Error),
    #[error("invalid system time")]
    InvalidSystemTime,
}

fn now_unix() -> Result<i64, MemoryError> {
    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|_| MemoryError::InvalidSystemTime)?
        .as_secs();
    i64::try_from(secs).map_err(|_| MemoryError::InvalidSystemTime)
}

/// Insert a memory record. All writes require a task_id for audit binding.
pub fn insert_memory(
    conn: &Connection,
    _task_id: &str,
    memory: &Memory,
) -> Result<(), MemoryError> {
    let now = now_unix()?;

    let mut stmt = conn.prepare(
        "INSERT INTO memories (
            id, project_id, agent_id, scope, content, tags, importance,
            created_at, updated_at, last_access
        ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
    )?;

    stmt.execute(params![
        memory.id,
        memory.project_id,
        memory.agent_id,
        memory.scope,
        memory.content,
        memory.tags,
        memory.importance,
        now,
        now,
        now,
    ])?;

    Ok(())
}

/// Insert a decision record.
pub fn insert_decision(
    conn: &Connection,
    task_id: &str,
    decision: &Decision,
) -> Result<(), MemoryError> {
    let now = now_unix()?;

    let mut stmt = conn.prepare(
        "INSERT INTO decisions (
            id, project_id, agent_id, task_id, summary, rationale,
            context_refs, outcome, created_at
        ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
    )?;

    stmt.execute(params![
        decision.id,
        decision.project_id,
        decision.agent_id,
        task_id,
        decision.summary,
        decision.rationale,
        decision.context_refs,
        decision.outcome,
        now,
    ])?;

    Ok(())
}

/// Update (or insert) an agent state record.
/// Note: task_id is required for audit trail per protocol, but state updates
/// are idempotent upserts on (agent_id, project_id) composite key.
pub fn update_agent_state(
    conn: &Connection,
    _task_id: &str,
    state: &AgentState,
) -> Result<(), MemoryError> {
    let now = now_unix()?;

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
        state.state_json,
        state.session_id,
        now,
    ])?;

    Ok(())
}
