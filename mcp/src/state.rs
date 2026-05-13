use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use rusqlite::Connection;

/// Shared MCP server state — cheap to clone (all heap data is Arc-wrapped).
#[derive(Clone)]
pub struct McpState {
    /// Project root directory (CWD when the server started).
    pub project_root: PathBuf,
    /// Path to `agents/models.yaml`.
    pub models_yaml_path: PathBuf,
    /// Path to `skycode.db`.
    pub db_path: PathBuf,
    /// API key for mutating tool auth (loaded from `.skycode/api.key`).
    pub api_key: Arc<String>,
    /// Shared database connection (single-writer serialised through mutex).
    pub conn: Arc<Mutex<Connection>>,
}
