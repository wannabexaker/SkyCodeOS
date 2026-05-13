use std::net::SocketAddr;

use axum::{extract::State, routing::post, Json, Router};
use serde_json::Value;
use tower_http::cors::CorsLayer;

use crate::proto::RpcRequest;
use crate::state::McpState;

/// Run the MCP server in Streamable-HTTP (POST /mcp) transport mode.
///
/// This is the mode used for LAN / Tailscale access from Claude Desktop (remote),
/// Cursor (HTTP transport), and SkaiRPG.  One JSON-RPC request per HTTP POST,
/// one JSON-RPC response body returned immediately.
pub async fn run_sse(
    state: McpState,
    host: &str,
    port: u16,
) -> Result<(), Box<dyn std::error::Error>> {
    let app = Router::new()
        .route("/mcp", post(mcp_handler))
        .layer(CorsLayer::permissive())
        .with_state(state);

    let addr: SocketAddr = format!("{host}:{port}").parse()?;
    println!("SkyCodeOS MCP server listening on http://{addr}/mcp");
    println!("  POST /mcp  (JSON-RPC 2.0)");

    let listener = tokio::net::TcpListener::bind(addr).await?;
    axum::serve(listener, app).await?;
    Ok(())
}

/// POST /mcp — receive a JSON-RPC request, dispatch it, return the response.
///
/// Blocking work (DB queries, git ops) is offloaded to the blocking thread pool
/// via `spawn_blocking` so the Tokio reactor is not starved.
async fn mcp_handler(State(state): State<McpState>, Json(req): Json<RpcRequest>) -> Json<Value> {
    let result = tokio::task::spawn_blocking(move || crate::handler::handle_request(req, &state))
        .await
        .unwrap_or_else(|_| {
            Some(crate::proto::RpcResponse::err(
                serde_json::Value::Null,
                -32603,
                "Internal error: handler panicked",
            ))
        });

    match result {
        Some(resp) => Json(serde_json::to_value(resp).unwrap_or_default()),
        // Notifications return HTTP 200 with an empty body.
        None => Json(serde_json::json!({})),
    }
}
