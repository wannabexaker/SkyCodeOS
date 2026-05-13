/// MCP tool-list value, conforming to the `tools/list` response schema.
pub fn tool_list() -> serde_json::Value {
    serde_json::json!([
        {
            "name": "list_models",
            "description": "List available AI models from models.yaml",
            "inputSchema": {
                "type": "object",
                "properties": {},
                "required": []
            }
        },
        {
            "name": "get_agent_state",
            "description": "Get current agent status, active model, and test_command",
            "inputSchema": {
                "type": "object",
                "properties": {},
                "required": []
            }
        },
        {
            "name": "get_diff",
            "description": "Fetch a DiffProposal by diff_id",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "diff_id": {
                        "type": "string",
                        "description": "UUID of the diff proposal"
                    }
                },
                "required": ["diff_id"]
            }
        },
        {
            "name": "search_memory",
            "description": "FTS5 memory search, returns ranked chunks",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "query": { "type": "string" },
                    "limit": { "type": "integer", "default": 10 }
                },
                "required": ["query"]
            }
        },
        {
            "name": "approve_diff",
            "description": "Create and sign an ApprovalToken for a diff. Requires api_key.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "api_key":  { "type": "string" },
                    "diff_id":  { "type": "string" },
                    "agent_id": { "type": "string" },
                    "task_id":  { "type": "string" }
                },
                "required": ["api_key", "diff_id", "agent_id", "task_id"]
            }
        },
        {
            "name": "apply_diff",
            "description": "Apply a single approved diff. Requires api_key.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "api_key":  { "type": "string" },
                    "diff_id":  { "type": "string" },
                    "token":    { "type": "object" },
                    "agent_id": { "type": "string" },
                    "task_id":  { "type": "string" }
                },
                "required": ["api_key", "diff_id", "token", "agent_id", "task_id"]
            }
        },
        {
            "name": "apply_diff_set",
            "description": "Atomic multi-diff apply with stash recovery. Requires api_key.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "api_key":  { "type": "string" },
                    "set_id":   { "type": "string" },
                    "tokens":   { "type": "array", "items": { "type": "object" } },
                    "agent_id": { "type": "string" },
                    "task_id":  { "type": "string" }
                },
                "required": ["api_key", "set_id", "tokens", "agent_id", "task_id"]
            }
        },
        {
            "name": "run_verify",
            "description": "Run test_command against current repo state. Requires api_key.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "api_key":      { "type": "string" },
                    "timeout_secs": { "type": "integer", "default": 60 }
                },
                "required": ["api_key"]
            }
        }
    ])
}
