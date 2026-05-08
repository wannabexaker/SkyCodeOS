use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ModelPolicy {
    pub preferred: String,
    pub fallback: String,
    pub profile: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SkyCoreConstraints {
    pub max_output_tokens: i32,
    pub stream: Option<bool>,
    pub stop: Option<Vec<String>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SkyCoreRequest {
    pub skycore_version: String,
    pub task_id: String,
    pub agent_id: String,
    pub goal: String,
    pub context_refs: Vec<String>,
    pub tools_allowed: Vec<String>,
    pub model_policy: ModelPolicy,
    pub output_contract: String,
    pub constraints: SkyCoreConstraints,
}
