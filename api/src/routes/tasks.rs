use std::time::{SystemTime, UNIX_EPOCH};

use axum::extract::State;
use axum::http::{HeaderMap, StatusCode};
use axum::response::Json;
use rusqlite::params;
use serde_json::{json, Value};
use uuid::Uuid;

use skycode_contracts::sky_task::{SkyTaskRequest, SkyTaskResponse};

use crate::state::AppState;

pub async fn create_task(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(request): Json<SkyTaskRequest>,
) -> Result<Json<SkyTaskResponse>, (StatusCode, Json<Value>)> {
    let key = headers
        .get("x-api-key")
        .and_then(|value| value.to_str().ok());

    if key != Some(state.api_key.as_str()) {
        return Err(error(
            StatusCode::UNAUTHORIZED,
            "missing or invalid X-Api-Key",
        ));
    }

    if request.agent_id.trim().is_empty() {
        return Err(error(StatusCode::BAD_REQUEST, "agent_id must be non-empty"));
    }

    if request.goal.trim().is_empty() {
        return Err(error(StatusCode::BAD_REQUEST, "goal must be non-empty"));
    }

    let task_id = Uuid::new_v4().to_string();
    let mode = request.mode.clone().unwrap_or_else(|| "diff".to_string());
    let status = "accepted".to_string();
    let created_at = now_unix()?;
    let external_ref = serialize_external_ref(request.external_ref.as_ref())?;
    let event_payload = json!({
        "agent_id": request.agent_id,
        "goal": request.goal,
        "mode": mode,
        "quest_id": request.quest_id,
        "guild_id": request.guild_id,
        "external_ref": request.external_ref,
    });
    let output_json = serde_json::to_string(&event_payload).map_err(|e| {
        error(
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("json error: {e}"),
        )
    })?;

    let conn = state
        .conn
        .lock()
        .map_err(|_| error(StatusCode::INTERNAL_SERVER_ERROR, "database lock poisoned"))?;

    conn.execute(
        "INSERT INTO submitted_tasks (
            id, agent_id, goal, mode, status, quest_id, guild_id, external_ref, created_at
         ) VALUES (
            ?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9
         )",
        params![
            task_id,
            request.agent_id,
            request.goal,
            mode,
            status,
            request.quest_id,
            request.guild_id,
            external_ref,
            created_at,
        ],
    )
    .map_err(|e| {
        error(
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("database error: {e}"),
        )
    })?;

    conn.execute(
        "INSERT INTO tool_events (
            id, task_id, agent_id, event_type, tool_name,
            inputs_hash, inputs_json, output_hash, output_json,
            approval_token_id, diff_id, profile_name, created_at
         ) VALUES (
            ?1, ?2, ?3, ?4, NULL,
            NULL, NULL, NULL, ?5,
            NULL, NULL, NULL, ?6
         )",
        params![
            Uuid::new_v4().to_string(),
            task_id,
            request.agent_id,
            "agent.turn.started",
            output_json,
            created_at,
        ],
    )
    .map_err(|e| {
        error(
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("database error: {e}"),
        )
    })?;

    Ok(Json(SkyTaskResponse {
        events_url: format!("/v1/events?task_id={task_id}"),
        task_id,
        status,
    }))
}

fn serialize_external_ref(
    value: Option<&Value>,
) -> Result<Option<String>, (StatusCode, Json<Value>)> {
    match value {
        Some(value) => serde_json::to_string(value).map(Some).map_err(|e| {
            error(
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("json error: {e}"),
            )
        }),
        None => Ok(None),
    }
}

fn now_unix() -> Result<i64, (StatusCode, Json<Value>)> {
    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|_| error(StatusCode::INTERNAL_SERVER_ERROR, "invalid system time"))?
        .as_secs();

    i64::try_from(secs).map_err(|_| error(StatusCode::INTERNAL_SERVER_ERROR, "invalid system time"))
}

fn error(status: StatusCode, message: impl Into<String>) -> (StatusCode, Json<Value>) {
    (
        status,
        Json(json!({
            "error": message.into(),
        })),
    )
}
