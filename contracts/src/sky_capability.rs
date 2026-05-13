use serde::{Deserialize, Serialize};

use crate::sky_event::SkyEventType;

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct SkyCapabilityInfo {
    pub engine_id: String,
    pub protocols: Vec<String>,
    pub supports_tools: bool,
    pub supports_repo_writes: bool,
    pub requires_approval_token: bool,
    pub local_first: bool,
    pub network_required: bool,
    pub mcp_tool_names: Vec<String>,
    pub event_types: Vec<String>,
}

impl Default for SkyCapabilityInfo {
    fn default() -> Self {
        Self {
            engine_id: "skycodeos-local".to_string(),
            protocols: vec![
                "openai".to_string(),
                "mcp".to_string(),
                "events".to_string(),
            ],
            supports_tools: true,
            supports_repo_writes: true,
            requires_approval_token: true,
            local_first: true,
            network_required: false,
            mcp_tool_names: vec![
                "list_models".to_string(),
                "get_agent_state".to_string(),
                "get_diff".to_string(),
                "search_memory".to_string(),
                "approve_diff".to_string(),
                "apply_diff".to_string(),
                "apply_diff_set".to_string(),
                "run_verify".to_string(),
            ],
            event_types: SkyEventType::all_names()
                .into_iter()
                .map(str::to_string)
                .collect(),
        }
    }
}
