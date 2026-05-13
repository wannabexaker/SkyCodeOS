use std::sync::{Arc, Mutex};

use clap::Args;
use rusqlite::Connection;

use skycode_mcp::state::McpState;
use skycode_orchestrator::db::migrations::run_migrations;

#[derive(Debug, Args)]
pub struct McpArgs {
    /// Run HTTP server instead of stdio (for LAN / Tailscale access).
    #[arg(long)]
    pub sse: bool,

    /// Host to bind for HTTP mode (default: 0.0.0.0 — LAN accessible).
    #[arg(long, default_value = "0.0.0.0")]
    pub host: String,

    /// Port for HTTP mode (default: 11435).
    #[arg(long, default_value_t = 11435u16)]
    pub port: u16,
}

pub fn run(args: &McpArgs) -> Result<(), Box<dyn std::error::Error>> {
    let project_root = std::env::current_dir()?;

    // Load API key (must already exist — created by `scos serve`).
    let key_path = project_root.join(".skycode").join("api.key");
    let api_key = if key_path.exists() {
        std::fs::read_to_string(&key_path)?.trim().to_string()
    } else {
        return Err("No API key found — run `scos serve` first to generate one".into());
    };

    // Open database and run pending migrations.
    let db_path = project_root.join("skycode.db");
    let conn = Connection::open(&db_path)?;
    let migrations_dir = project_root.join("memory").join("migrations");
    if migrations_dir.exists() {
        run_migrations(&conn, &migrations_dir)?;
    }

    let state = McpState {
        models_yaml_path: project_root.join("agents").join("models.yaml"),
        db_path: db_path.clone(),
        project_root: project_root.clone(),
        api_key: Arc::new(api_key),
        conn: Arc::new(Mutex::new(conn)),
    };

    if args.sse {
        // HTTP transport — needs a Tokio runtime.
        let rt = tokio::runtime::Runtime::new()?;
        rt.block_on(skycode_mcp::sse::run_sse(state, &args.host, args.port))?;
    } else {
        // stdio transport — blocking, no runtime needed.
        skycode_mcp::stdio::run_stdio(state)?;
    }

    Ok(())
}
