use axum::{
    body::Body,
    extract::State,
    http::{Request, StatusCode},
    middleware::Next,
    response::Response,
};

use crate::state::AppState;

pub async fn require_api_key(
    State(state): State<AppState>,
    req: Request<Body>,
    next: Next,
) -> Result<Response, StatusCode> {
    // /health is always open
    if req.uri().path() == "/health" {
        return Ok(next.run(req).await);
    }

    let bearer = req
        .headers()
        .get("Authorization")
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "));

    if bearer == Some(state.api_key.as_str()) {
        return Ok(next.run(req).await);
    }

    let x_api_key = req.headers().get("X-Api-Key").and_then(|v| v.to_str().ok());

    match x_api_key {
        Some(key) if req.uri().path() == "/v1/tasks" && key == state.api_key.as_str() => {
            Ok(next.run(req).await)
        }
        _ => Err(StatusCode::UNAUTHORIZED),
    }
}
