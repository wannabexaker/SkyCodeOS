use axum::Json;
use serde_json::{json, Value};

/// GET /health - no auth required, no DB required.
pub async fn handler() -> Json<Value> {
    Json(json!({ "status": "ok" }))
}
