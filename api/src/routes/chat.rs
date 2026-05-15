use std::collections::HashMap;
use std::convert::Infallible;
use std::fmt::Display;
use std::time::Duration;

use axum::body::Body;
use axum::extract::State;
use axum::http::{header::CONTENT_TYPE, HeaderValue, StatusCode};
use axum::response::sse::{Event, KeepAlive, Sse};
use axum::response::{IntoResponse, Response};
use axum::Json;
use futures_util::stream::StreamExt;
use futures_util::stream::{self};
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
        let stream = upstream_response
            .bytes_stream()
            .map(SseStreamInput::Chunk)
            .chain(stream::once(async { SseStreamInput::End }))
            .scan(SseLineBuffer::default(), |state, input| {
                futures_util::future::ready(Some(state.handle(input)))
            })
            .flat_map(stream::iter);

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

enum SseStreamInput<T, E> {
    Chunk(Result<T, E>),
    End,
}

#[derive(Default)]
struct SseLineBuffer {
    buffer: String,
    closed: bool,
}

impl SseLineBuffer {
    fn handle<T, E>(&mut self, input: SseStreamInput<T, E>) -> Vec<Result<Event, Infallible>>
    where
        T: AsRef<[u8]>,
        E: Display,
    {
        let mut events = Vec::new();

        if self.closed {
            return events;
        }

        match input {
            SseStreamInput::Chunk(Ok(bytes)) => {
                self.buffer
                    .push_str(String::from_utf8_lossy(bytes.as_ref()).as_ref());
                self.drain_complete_lines(&mut events);
            }
            SseStreamInput::Chunk(Err(err)) => {
                self.closed = true;
                self.buffer.clear();
                events.push(Ok(upstream_stream_error_event(&err.to_string())));
            }
            SseStreamInput::End => {
                self.flush_partial_line(&mut events);
                self.closed = true;
            }
        }

        events
    }

    fn drain_complete_lines(&mut self, events: &mut Vec<Result<Event, Infallible>>) {
        while let Some(newline_idx) = self.buffer.find('\n') {
            let mut raw_line = self.buffer.drain(..=newline_idx).collect::<String>();
            if raw_line.ends_with('\n') {
                raw_line.pop();
            }
            if raw_line.ends_with('\r') {
                raw_line.pop();
            }
            push_sse_line_event(&raw_line, events);
        }
    }

    fn flush_partial_line(&mut self, events: &mut Vec<Result<Event, Infallible>>) {
        let line = self.buffer.trim_end_matches('\r').to_string();
        self.buffer.clear();
        push_sse_line_event(&line, events);
    }
}

fn push_sse_line_event(line: &str, events: &mut Vec<Result<Event, Infallible>>) {
    // This proxy forwards only `data:` frames. Empty lines, comments
    // (`:keepalive`), and SSE metadata (`event:`, `id:`, `retry:`) are skipped.
    if line.is_empty()
        || line.starts_with(':')
        || line.starts_with("event:")
        || line.starts_with("id:")
        || line.starts_with("retry:")
    {
        return;
    }

    if let Some(data) = line.strip_prefix("data:") {
        events.push(Ok(Event::default().data(data.trim_start().to_string())));
    }
}

fn upstream_stream_error_event(message: &str) -> Event {
    let payload = match serde_json::to_string(&json!({
        "error": {
            "message": message,
            "type": "api_error",
            "code": "upstream_stream_error"
        }
    })) {
        Ok(payload) => payload,
        Err(_) => {
            "{\"error\":{\"message\":\"upstream stream error\",\"type\":\"api_error\",\"code\":\"upstream_stream_error\"}}"
                .to_string()
        }
    };

    Event::default().data(payload)
}
