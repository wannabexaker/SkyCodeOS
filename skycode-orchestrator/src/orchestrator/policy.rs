use rusqlite::Connection;
use thiserror::Error;

use skycode_agent::agent::intent::AgentIntent;
use skycode_memory::memory::store::{insert_decision, Decision, MemoryError};

#[derive(Debug, Error)]
pub enum PolicyError {
    #[error("tool is forbidden by doctrine: {0}")]
    ForbiddenTool(String),
    #[error("tool requires approval token: {0}")]
    ApprovalRequired(String),
    #[error("failed writing decision: {0}")]
    DecisionWrite(#[from] MemoryError),
}

pub fn enforce_doctrine(intent: &AgentIntent, tool: &str) -> Result<(), PolicyError> {
    if intent
        .constraints
        .iter()
        .any(|c| c == &format!("must_never:{tool}"))
    {
        return Err(PolicyError::ForbiddenTool(tool.to_string()));
    }

    if matches!(tool, "file_write" | "file_delete" | "patch_apply") {
        return Err(PolicyError::ApprovalRequired(tool.to_string()));
    }

    Ok(())
}

pub fn write_decision(conn: &Connection, decision: &Decision) -> Result<(), PolicyError> {
    insert_decision(conn, &decision.task_id, decision)?;
    Ok(())
}
