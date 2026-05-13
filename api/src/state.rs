use rusqlite::Connection;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

#[derive(Clone)]
pub struct AppState {
    /// The validated API key (64-char hex string).
    pub api_key: Arc<String>,
    /// Path to `agents/models.yaml`.
    pub models_yaml_path: PathBuf,
    /// Path to `skycode.db`.
    pub db_path: PathBuf,
    /// Project root directory (where `.skycode/` lives).
    pub project_root: PathBuf,
    /// Shared HTTP client for upstream proxy.
    pub http_client: reqwest::Client,
    /// Shared database connection for read-only queries.
    pub conn: Arc<Mutex<Connection>>,
}
