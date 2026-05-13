use std::collections::HashMap;
use std::convert::Infallible;
use std::time::Duration;

use axum::body::Body;
use axum::extract::State;
use axum::http::{header::CONTENT_TYPE, HeaderValue, StatusCode};
use axum::response::sse::{Event, KeepAlive, Sse};
use axum::response::{IntoResponse, Response};
use axum::Json;
use futures_util::stream::StreamExt;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use skycode_inference::inference::registry::{ModelRegistry, ModelRuntime};

use crate::error::ApiError;
use crate::state::AppState;

#[derive(Debug, Deserialize, Serialize)]
pub struct ChatRequest {
    pub model: String,
    pub messages: Vec<serde_json::Value>,
    pub stream: Option<bool>,
    // pass all other fields through unchanged
    #[serde(flatten)]
    pub extra: HashMap<String, serde_json::Value>,
}

pub async fn handler(
    State(state): State<AppState>,
    Json(request): Json<ChatRequest>,
) -> Result<Response, (StatusCode, Json<Value>)> {
    let content = std::fs::read_to_string(&state.models_yaml_path).map_err(|e| {
        let (s, j) = ApiError::internal(format!("cannot read models.yaml: {e}"));
        (
            s,
            Json(serde_json::to_value(j.0).unwrap_or_else(|_| json!({}))),
        )
    })?;

    let registry = ModelRegistry::from_yaml(&content).map_err(|e| {
        let (s, j) = ApiError::internal(format!("cannot parse models.yaml: {e}"));
        (
            s,
            Json(serde_json::to_value(j.0).unwrap_or_else(|_| json!({}))),
        )
    })?;

    let model = registry
        .models
        .iter()
        .find(|m| m.name == request.model && m.enabled)
        .ok_or_else(|| {
            let (s, j) = ApiError::not_found("model not found or disabled");
            (
                s,
                Json(serde_json::to_value(j.0).unwrap_or_else(|_| json!({}))),
            )
        })?;

    let upstream_url = match model.runtime {
        ModelRuntime::LocalGguf => {
            format!("http://127.0.0.1:{}/v1/chat/completions", model.port)
        }
        ModelRuntime::OpenaiCompatible => {
            format!("{}/v1/chat/completions", model.path.trim_end_matches('/'))
        }
    };

    let client = &state.http_client;
    let upstream_response = client
        .post(&upstream_url)
        .json(&request)
        .send()
        .await
        .map_err(|_| upstream_unreachable())?;

    if request.stream == Some(true) {
        let stream = upstream_response.bytes_stream().flat_map(|chunk_result| {
            let mut events: Vec<Result<Event, Infallible>> = Vec::new();

            if let Ok(bytes) = chunk_result {
                let text = String::from_utf8_lossy(&bytes);
                for raw_line in text.split('\n') {
                    let line = raw_line.trim_end_matches('\r');
                    if let Some(data) = line.strip_prefix("data:") {
                        events.push(Ok(Event::default().data(data.trim_start().to_string())));
                    }
                }
            }

            futures_util::stream::iter(events)
        });

        let sse = Sse::new(stream).keep_alive(KeepAlive::new().interval(Duration::from_secs(15)));
        return Ok(sse.into_response());
    }

    let status = match StatusCode::from_u16(upstream_response.status().as_u16()) {
        Ok(code) => code,
        Err(_) => StatusCode::BAD_GATEWAY,
    };

    let content_type = upstream_response
        .headers()
        .get(reqwest::header::CONTENT_TYPE)
        .cloned();

    let body = upstream_response
        .bytes()
        .await
        .map_err(|_| upstream_unreachable())?;

    let mut response = Response::new(Body::from(body));
    *response.status_mut() = status;

    if let Some(value) = content_type {
        if let Ok(value_str) = value.to_str() {
            if let Ok(header_value) = HeaderValue::from_str(value_str) {
                response.headers_mut().insert(CONTENT_TYPE, header_value);
            }
        }
    } else {
        response
            .headers_mut()
            .insert(CONTENT_TYPE, HeaderValue::from_static("application/json"));
    }

    Ok(response)
}

fn upstream_unreachable() -> (StatusCode, Json<Value>) {
    (
        StatusCode::BAD_GATEWAY,
        Json(json!({
            "error": {
                "message": "model backend unreachable",
                "type": "api_error",
                "code": "upstream_error"
            }
        })),
    )
}
