use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};

#[derive(Serialize, Deserialize, Clone, Debug)]
pub enum SkyEventType {
    AgentTurnStarted,
    AgentTurnCompleted,
    ModelInvoked,
    ToolRequested,
    ToolCompleted,
    DiffProposed,
    DiffApproved,
    DiffApplied,
    VerifyPassed,
    ApplyUnverified,
    MemoryRetrieved,
    SecurityBlocked,
}

impl SkyEventType {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::AgentTurnStarted => "agent.turn.started",
            Self::AgentTurnCompleted => "agent.turn.completed",
            Self::ModelInvoked => "model.invoked",
            Self::ToolRequested => "tool.requested",
            Self::ToolCompleted => "tool.completed",
            Self::DiffProposed => "diff.proposed",
            Self::DiffApproved => "diff.approved",
            Self::DiffApplied => "diff.applied",
            Self::VerifyPassed => "verify.passed",
            Self::ApplyUnverified => "apply.unverified",
            Self::MemoryRetrieved => "memory.retrieved",
            Self::SecurityBlocked => "security.blocked",
        }
    }

    pub fn all_names() -> Vec<&'static str> {
        vec![
            Self::AgentTurnStarted.as_str(),
            Self::AgentTurnCompleted.as_str(),
            Self::ModelInvoked.as_str(),
            Self::ToolRequested.as_str(),
            Self::ToolCompleted.as_str(),
            Self::DiffProposed.as_str(),
            Self::DiffApproved.as_str(),
            Self::DiffApplied.as_str(),
            Self::VerifyPassed.as_str(),
            Self::ApplyUnverified.as_str(),
            Self::MemoryRetrieved.as_str(),
            Self::SecurityBlocked.as_str(),
        ]
    }
}

pub fn compute_event_id(task_id: &str, cursor: i64) -> String {
    let mut hasher = Sha256::new();
    hasher.update(task_id.as_bytes());
    hasher.update(b":");
    hasher.update(cursor.to_string().as_bytes());
    let digest = hasher.finalize();

    let mut out = String::with_capacity(digest.len() * 2);
    for byte in digest {
        out.push(hex_char((byte >> 4) & 0x0f));
        out.push(hex_char(byte & 0x0f));
    }
    out
}

fn hex_char(value: u8) -> char {
    match value {
        0..=9 => (b'0' + value) as char,
        _ => (b'a' + (value - 10)) as char,
    }
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct SkyEvent {
    pub event_id: String,
    pub source: String,
    pub cursor: i64,
    pub task_id: String,
    pub agent_id: String,
    pub project_id: String,
    pub quest_id: Option<String>,
    pub event_type: String,
    pub payload: Value,
    pub created_at: String,
}

impl SkyEvent {
    pub fn new(
        source: impl Into<String>,
        cursor: i64,
        task_id: impl Into<String>,
        agent_id: impl Into<String>,
        project_id: impl Into<String>,
        quest_id: Option<String>,
        event_type: SkyEventType,
        payload: Value,
        created_at: impl Into<String>,
    ) -> Self {
        let task_id = task_id.into();
        Self {
            event_id: compute_event_id(&task_id, cursor),
            source: source.into(),
            cursor,
            task_id,
            agent_id: agent_id.into(),
            project_id: project_id.into(),
            quest_id,
            event_type: event_type.as_str().to_string(),
            payload,
            created_at: created_at.into(),
        }
    }
}
