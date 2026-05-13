use std::path::Path;

use axum::extract::{Path as AxumPath, State};
use axum::http::StatusCode;
use axum::Json;
use ring::signature::{Ed25519KeyPair, KeyPair};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use uuid::Uuid;

use skycode_core::approval::token::ApprovalToken;
use skycode_core::approval::validator::register_signing_key;
use skycode_tools::tools::apply::apply_diff;
use skycode_tools::tools::diff::DiffProposal;

use crate::error::ApiError;
use crate::state::AppState;

// ============================================================================
// Data Structures
// ============================================================================

#[derive(Serialize)]
pub struct DiffObject {
    pub id: String,
    pub task_id: String,
    pub project_id: String,
    pub affected_files: Vec<String>,
    pub created_at: i64,
}

#[derive(Serialize)]
pub struct DiffList {
    pub object: &'static str,
    pub data: Vec<DiffObject>,
}

#[derive(Deserialize)]
pub struct ListQuery {
    pub task_id: String,
}

#[derive(Deserialize)]
pub struct ApproveRequest {
    pub agent_id: String,
    pub task_id: String,
}

#[derive(Serialize)]
pub struct ApproveResponse {
    pub token: ApprovalToken,
}

#[derive(Deserialize)]
pub struct ApplyRequest {
    pub token: ApprovalToken,
    pub agent_id: String,
    pub task_id: String,
    pub project_id: String,
}

#[derive(Serialize)]
pub struct ApplyResponse {
    pub applied: bool,
    pub diff_id: String,
}

// ============================================================================
// Helpers
// ============================================================================

/// Load or create Ed25519 signing key from `.skycode/keys/approval_ed25519.pk8`.
/// Returns 404 if file not found.
fn load_signing_key(project_root: &Path) -> Result<Ed25519KeyPair, (StatusCode, Json<Value>)> {
    let key_path = project_root
        .join(".skycode")
        .join("keys")
        .join("approval_ed25519.pk8");

    let key_bytes = std::fs::read(&key_path).map_err(|_| {
        let (s, j) = ApiError::not_found("no signing key — run `scos approve` once from CLI first");
        (
            s,
            Json(serde_json::to_value(j.0).unwrap_or_else(|_| json!({}))),
        )
    })?;

    Ed25519KeyPair::from_pkcs8(&key_bytes).map_err(|_| {
        let (s, j) = ApiError::internal("invalid signing key");
        (
            s,
            Json(serde_json::to_value(j.0).unwrap_or_else(|_| json!({}))),
        )
    })
}

/// Persist token to `.skycode/tokens/<diff_id>.json`.
fn persist_token(
    project_root: &Path,
    diff_id: &str,
    token: &ApprovalToken,
) -> Result<(), (StatusCode, Json<Value>)> {
    let tokens_dir = project_root.join(".skycode").join("tokens");
    std::fs::create_dir_all(&tokens_dir).map_err(|e| {
        let (s, j) = ApiError::internal(format!("cannot create tokens dir: {e}"));
        (
            s,
            Json(serde_json::to_value(j.0).unwrap_or_else(|_| json!({}))),
        )
    })?;

    let token_path = tokens_dir.join(format!("{}.json", diff_id));
    let token_json = serde_json::to_string(token).map_err(|e| {
        let (s, j) = ApiError::internal(format!("cannot serialize token: {e}"));
        (
            s,
            Json(serde_json::to_value(j.0).unwrap_or_else(|_| json!({}))),
        )
    })?;

    std::fs::write(&token_path, token_json).map_err(|e| {
        let (s, j) = ApiError::internal(format!("cannot persist token: {e}"));
        (
            s,
            Json(serde_json::to_value(j.0).unwrap_or_else(|_| json!({}))),
        )
    })?;

    Ok(())
}

fn extract_file_path_from_patch(patch_unified: &str) -> String {
    for line in patch_unified.lines() {
        if let Some(path) = line.strip_prefix("--- a/") {
            return path.to_string();
        }
    }
    "unknown".to_string()
}

// ============================================================================
// Handlers
// ============================================================================

/// GET /v1/diffs?task_id=<id>
pub async fn list_handler(
    State(state): State<AppState>,
    axum::extract::Query(params): axum::extract::Query<ListQuery>,
) -> Result<Json<DiffList>, (StatusCode, Json<Value>)> {
    let conn = state.conn.lock().map_err(|_| {
        let (s, j) = ApiError::internal("database lock poisoned");
        (
            s,
            Json(serde_json::to_value(j.0).unwrap_or_else(|_| json!({}))),
        )
    })?;

    let mut stmt = conn
        .prepare(
            "SELECT id, task_id, project_id, affected_files_json, created_at \
             FROM diff_proposals \
             WHERE task_id = ?1 \
             ORDER BY created_at ASC",
        )
        .map_err(|e| {
            let (s, j) = ApiError::internal(format!("database error: {e}"));
            (
                s,
                Json(serde_json::to_value(j.0).unwrap_or_else(|_| json!({}))),
            )
        })?;

    let diffs = stmt
        .query_map([&params.task_id], |row| {
            let id: String = row.get(0)?;
            let task_id: String = row.get(1)?;
            let project_id: String = row.get(2)?;
            let affected_files_json: String = row.get(3)?;
            let created_at: i64 = row.get(4)?;

            let affected_files: Vec<String> =
                serde_json::from_str(&affected_files_json).unwrap_or_default();

            Ok(DiffObject {
                id,
                task_id,
                project_id,
                affected_files,
                created_at,
            })
        })
        .map_err(|e| {
            let (s, j) = ApiError::internal(format!("query error: {e}"));
            (
                s,
                Json(serde_json::to_value(j.0).unwrap_or_else(|_| json!({}))),
            )
        })?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| {
            let (s, j) = ApiError::internal(format!("row error: {e}"));
            (
                s,
                Json(serde_json::to_value(j.0).unwrap_or_else(|_| json!({}))),
            )
        })?;

    Ok(Json(DiffList {
        object: "list",
        data: diffs,
    }))
}

/// POST /v1/diffs/:diff_id/approve
pub async fn approve_handler(
    State(state): State<AppState>,
    AxumPath(diff_id): AxumPath<String>,
    Json(request): Json<ApproveRequest>,
) -> Result<Json<ApproveResponse>, (StatusCode, Json<Value>)> {
    // Validate diff exists and get project_id
    let conn = state.conn.lock().map_err(|_| {
        let (s, j) = ApiError::internal("database lock poisoned");
        (
            s,
            Json(serde_json::to_value(j.0).unwrap_or_else(|_| json!({}))),
        )
    })?;

    let project_id: String = conn
        .query_row(
            "SELECT project_id FROM diff_proposals WHERE id = ?1 LIMIT 1",
            [&diff_id],
            |row| row.get(0),
        )
        .map_err(|_| {
            let (s, j) = ApiError::not_found("diff not found");
            (
                s,
                Json(serde_json::to_value(j.0).unwrap_or_else(|_| json!({}))),
            )
        })?;

    drop(conn); // Release lock before loading key

    // Load signing key
    let key_pair = load_signing_key(&state.project_root)?;

    // Generate nonce
    let nonce = Uuid::new_v4().to_string();

    // Create signed token
    let token = ApprovalToken::create_signed(
        &project_id,
        &diff_id,
        &request.agent_id,
        &request.agent_id,
        &nonce,
        &key_pair,
    )
    .map_err(|e| {
        let (s, j) = ApiError::internal(format!("token creation failed: {e}"));
        (
            s,
            Json(serde_json::to_value(j.0).unwrap_or_else(|_| json!({}))),
        )
    })?;

    // Register signing key in DB
    let pub_key_hex = hex::encode(key_pair.public_key().as_ref());
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64;

    let conn = state.conn.lock().map_err(|_| {
        let (s, j) = ApiError::internal("database lock poisoned");
        (
            s,
            Json(serde_json::to_value(j.0).unwrap_or_else(|_| json!({}))),
        )
    })?;

    register_signing_key(&conn, &request.agent_id, &pub_key_hex, now).map_err(|e| {
        let (s, j) = ApiError::internal(format!("register signing key failed: {e}"));
        (
            s,
            Json(serde_json::to_value(j.0).unwrap_or_else(|_| json!({}))),
        )
    })?;

    drop(conn); // Release lock before persisting

    // Persist token
    persist_token(&state.project_root, &diff_id, &token)?;

    Ok(Json(ApproveResponse { token }))
}

/// POST /v1/diffs/:diff_id/apply
pub async fn apply_handler(
    State(state): State<AppState>,
    AxumPath(diff_id): AxumPath<String>,
    Json(request): Json<ApplyRequest>,
) -> Result<Json<ApplyResponse>, (StatusCode, Json<Value>)> {
    // Query diff_proposals to get patch
    let conn = state.conn.lock().map_err(|_| {
        let (s, j) = ApiError::internal("database lock poisoned");
        (
            s,
            Json(serde_json::to_value(j.0).unwrap_or_else(|_| json!({}))),
        )
    })?;

    let (project_id_db, patch_unified): (String, String) = conn
        .query_row(
            "SELECT project_id, patch_unified \
             FROM diff_proposals WHERE id = ?1 LIMIT 1",
            [&diff_id],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .map_err(|_| {
            let (s, j) = ApiError::not_found("diff not found");
            (
                s,
                Json(serde_json::to_value(j.0).unwrap_or_else(|_| json!({}))),
            )
        })?;

    drop(conn); // Release lock before apply

    // Build DiffProposal from patch
    let file_path = extract_file_path_from_patch(&patch_unified);
    let parsed_diff_id = Uuid::parse_str(&diff_id).map_err(|_| {
        let (s, j) = ApiError::internal("invalid diff id");
        (
            s,
            Json(serde_json::to_value(j.0).unwrap_or_else(|_| json!({}))),
        )
    })?;

    let diff = DiffProposal {
        id: parsed_diff_id,
        project_id: project_id_db.clone(),
        diff_text: patch_unified,
        file_path,
        created_at: 0,
    };

    // Apply the diff
    let conn = state.conn.lock().map_err(|_| {
        let (s, j) = ApiError::internal("database lock poisoned");
        (
            s,
            Json(serde_json::to_value(j.0).unwrap_or_else(|_| json!({}))),
        )
    })?;

    apply_diff(
        &*conn,
        &request.token,
        &request.agent_id,
        &request.task_id,
        &state.project_root,
        &project_id_db,
        &diff,
    )
    .map_err(|e| {
        let (s, j) = ApiError::internal(format!("apply failed: {e}"));
        (
            s,
            Json(serde_json::to_value(j.0).unwrap_or_else(|_| json!({}))),
        )
    })?;

    drop(conn);

    Ok(Json(ApplyResponse {
        applied: true,
        diff_id,
    }))
}
