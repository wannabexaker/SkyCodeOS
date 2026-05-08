use ring::digest;
use rusqlite::{params, Connection};
use thiserror::Error;

/// Event types matching the CHECK constraint in docs/schemas.md exactly.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EventType {
    ToolRequested,
    DiffProposed,
    DiffApproved,
    DiffRejected,
    DiffApplied,
    DiffApplyFailed,
    RollbackRequested,
    RollbackApplied,
    RollbackFailed,
    PolicyDenied,
    SecretRedacted,
    ModelInvoked,
    ModelFailed,
    MemoryWritten,
    DecisionWritten,
    ContextBudgetEnforced,
    TrustCheckFailed,
    TuningRunStarted,
    TuningRunCompleted,
    MigrationDestructiveApplied,
}

impl EventType {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::ToolRequested => "tool_requested",
            Self::DiffProposed => "diff_proposed",
            Self::DiffApproved => "diff_approved",
            Self::DiffRejected => "diff_rejected",
            Self::DiffApplied => "diff_applied",
            Self::DiffApplyFailed => "diff_apply_failed",
            Self::RollbackRequested => "rollback_requested",
            Self::RollbackApplied => "rollback_applied",
            Self::RollbackFailed => "rollback_failed",
            Self::PolicyDenied => "policy_denied",
            Self::SecretRedacted => "secret_redacted",
            Self::ModelInvoked => "model_invoked",
            Self::ModelFailed => "model_failed",
            Self::MemoryWritten => "memory_written",
            Self::DecisionWritten => "decision_written",
            Self::ContextBudgetEnforced => "context_budget_enforced",
            Self::TrustCheckFailed => "trust_check_failed",
            Self::TuningRunStarted => "tuning_run_started",
            Self::TuningRunCompleted => "tuning_run_completed",
            Self::MigrationDestructiveApplied => "migration_destructive_applied",
        }
    }
}

/// Columns match docs/schemas.md tool_events DDL exactly.
/// `id` is a hex-encoded sha256 of the serialized payload (content-addressed).
#[derive(Debug, Clone)]
pub struct ToolEvent {
    pub id: String,
    pub task_id: String,
    pub agent_id: String,
    pub event_type: EventType,
    pub tool_name: Option<String>,
    pub inputs_hash: Option<String>,
    pub inputs_json: Option<String>,
    pub output_hash: Option<String>,
    pub output_json: Option<String>,
    pub approval_token_id: Option<String>,
    pub diff_id: Option<String>,
    pub profile_name: Option<String>,
    pub created_at: i64,
}

#[derive(Debug, Error)]
pub enum EventsError {
    #[error("database error: {0}")]
    Database(#[from] rusqlite::Error),
}

/// Returns hex-encoded sha256 of `payload`, suitable for use as the
/// content-addressed `id` of a `ToolEvent`.
pub fn content_id(payload: &[u8]) -> String {
    let hash = digest::digest(&digest::SHA256, payload);
    let mut out = String::with_capacity(hash.as_ref().len() * 2);
    for b in hash.as_ref() {
        out.push(hex_char((b >> 4) & 0x0f));
        out.push(hex_char(b & 0x0f));
    }
    out
}

fn hex_char(value: u8) -> char {
    match value {
        0..=9 => (b'0' + value) as char,
        _ => (b'a' + (value - 10)) as char,
    }
}

pub fn append_event(conn: &Connection, event: &ToolEvent) -> Result<(), EventsError> {
    let mut stmt = conn.prepare(
        "INSERT INTO tool_events (
            id, task_id, agent_id, event_type, tool_name,
            inputs_hash, inputs_json, output_hash, output_json,
            approval_token_id, diff_id, profile_name, created_at
        ) VALUES (
            ?1, ?2, ?3, ?4, ?5,
            ?6, ?7, ?8, ?9,
            ?10, ?11, ?12, ?13
        )",
    )?;

    stmt.execute(params![
        event.id,
        event.task_id,
        event.agent_id,
        event.event_type.as_str(),
        event.tool_name,
        event.inputs_hash,
        event.inputs_json,
        event.output_hash,
        event.output_json,
        event.approval_token_id,
        event.diff_id,
        event.profile_name,
        event.created_at,
    ])?;

    Ok(())
}
