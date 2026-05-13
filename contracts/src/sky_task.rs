use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct SkyTaskRequest {
    pub agent_id: String,
    pub goal: String,
    pub mode: Option<String>,
    pub quest_id: Option<String>,
    pub guild_id: Option<String>,
    pub external_ref: Option<serde_json::Value>,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct SkyTaskResponse {
    pub task_id: String,
    pub status: String,
    pub events_url: String,
}
