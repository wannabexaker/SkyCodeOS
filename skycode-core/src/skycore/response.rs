use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SkyCoreStatus {
    Ok,
    Error,
    NeedsApproval,
    NeedsTool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SkyCoreArtifact {
    pub kind: String,
    pub id: String,
    /// Unified diff. Preferred when provided by the model.
    #[serde(default)]
    pub patch_unified: Option<String>,
    /// Complete new file content. When present, the runtime computes the diff
    /// itself (more reliable than asking the model to produce a unified diff).
    #[serde(default)]
    pub new_content: Option<String>,
    /// Liberal bridge for model responses that emit file content under a
    /// generic `content` key. The orchestrator normalizes this into
    /// `new_content` for create/rewrite artifacts.
    #[serde(default)]
    pub content: Option<String>,
    #[serde(default)]
    pub affected_files: Option<Vec<String>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SkyCoreToolCall {
    pub tool: String,
    pub inputs: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SkyCoreError {
    pub code: String,
    pub message: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SkyCoreResponse {
    pub skycore_version: String,
    pub task_id: String,
    pub status: SkyCoreStatus,
    pub summary: String,
    pub artifacts: Vec<SkyCoreArtifact>,
    pub tool_calls_requested: Vec<SkyCoreToolCall>,
    pub requires_approval: bool,
    pub error: Option<SkyCoreError>,
}
