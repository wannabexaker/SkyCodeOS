use std::sync::{Arc, Mutex};

use clap::Args;
use rusqlite::Connection;

use skycode_api::state::AppState;
use skycode_orchestrator::db::migrations::run_migrations;

#[derive(Debug, Args)]
pub struct ServeArgs {
    /// Host to bind (default: 0.0.0.0 - LAN accessible).
    #[arg(long, default_value = "0.0.0.0")]
    pub host: String,

    /// Port to listen on (default: 11434, same as Ollama).
    #[arg(long, default_value_t = 11434)]
    pub port: u16,
}

pub fn run(args: &ServeArgs) -> Result<(), Box<dyn std::error::Error>> {
    let project_root = std::env::current_dir()?;
    let api_key = skycode_api::key::load_or_create(&project_root)?;

    println!("API key: {api_key}");
    println!("Store this key in SKYCODE_API_KEY on client machines.");

    // Open database and run migrations
    let db_path = project_root.join("skycode.db");
    let conn = Connection::open(&db_path)?;
    let migrations_dir = project_root.join("memory").join("migrations");
    if migrations_dir.exists() {
        run_migrations(&conn, &migrations_dir)?;
    }
    let conn = Arc::new(Mutex::new(conn));

    let state = AppState {
        api_key: Arc::new(api_key),
        models_yaml_path: project_root.join("agents").join("models.yaml"),
        db_path,
        project_root,
        http_client: reqwest::Client::new(),
        conn,
    };

    let rt = tokio::runtime::Runtime::new()?;
    rt.block_on(skycode_api::server::run(state, &args.host, args.port))?;
    Ok(())
}
