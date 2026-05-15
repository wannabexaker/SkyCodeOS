use std::future::Future;
use std::net::SocketAddr;

use axum::{
    middleware,
    routing::{get, post},
    Router,
};
use tower_http::cors::CorsLayer;

use crate::routes::{capabilities, chat, diffs, events, health, models, tasks};
use crate::state::AppState;

pub async fn run(state: AppState, host: &str, port: u16) -> Result<(), Box<dyn std::error::Error>> {
    run_with_shutdown(state, host, port, shutdown_signal()).await
}

pub async fn run_with_shutdown<S>(
    state: AppState,
    host: &str,
    port: u16,
    shutdown: S,
) -> Result<(), Box<dyn std::error::Error>>
where
    S: Future<Output = ()> + Send + 'static,
{
    let app = Router::new()
        .route("/health", get(health::handler))
        .route("/v1/models", get(models::handler))
        .route("/v1/capabilities", get(capabilities::handler))
        .route("/v1/chat/completions", post(chat::handler))
        .route("/v1/tasks", post(tasks::create_task))
        .route("/v1/diffs", get(diffs::list_handler))
        .route("/v1/diffs/{diff_id}/approve", post(diffs::approve_handler))
        .route("/v1/diffs/{diff_id}/apply", post(diffs::apply_handler))
        .route("/v1/events", get(events::handler))
        // auth middleware applies to all routes; health handler bypasses it internally
        .layer(middleware::from_fn_with_state(
            state.clone(),
            crate::auth::require_api_key,
        ))
        .layer(CorsLayer::permissive())
        .with_state(state);

    let addr: SocketAddr = format!("{host}:{port}").parse()?;
    println!("SkyCodeOS API listening on http://{addr}");
    println!("  GET  /health     (no auth)");
    println!("  GET  /v1/models");
    println!("  GET  /v1/capabilities");
    println!("  POST /v1/chat/completions");
    println!("  POST /v1/tasks");
    println!("  GET  /v1/diffs");
    println!("  POST /v1/diffs/:diff_id/approve");
    println!("  POST /v1/diffs/:diff_id/apply");
    println!("  GET  /v1/events");

    let listener = tokio::net::TcpListener::bind(addr).await?;
    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown)
        .await?;
    Ok(())
}

async fn shutdown_signal() {
    let _ = tokio::signal::ctrl_c().await;
}
