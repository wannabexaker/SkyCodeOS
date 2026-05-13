use axum::{extract::State, Json};
use serde::Serialize;
use serde_json::Value;

use skycode_inference::inference::registry::ModelRegistry;

use crate::error::ApiError;
use crate::state::AppState;

#[derive(Serialize)]
struct ModelObject {
    id: String,
    object: &'static str,
    owned_by: &'static str,
    created: u64,
}

#[derive(Serialize)]
pub struct ModelList {
    object: &'static str,
    data: Vec<ModelObject>,
}

pub async fn handler(
    State(state): State<AppState>,
) -> Result<Json<ModelList>, (axum::http::StatusCode, Json<Value>)> {
    let content = std::fs::read_to_string(&state.models_yaml_path).map_err(|e| {
        let (s, j) = ApiError::internal(format!("cannot read models.yaml: {e}"));
        (s, Json(serde_json::to_value(j.0).unwrap_or_default()))
    })?;

    let registry = ModelRegistry::from_yaml(&content).map_err(|e| {
        let (s, j) = ApiError::internal(format!("cannot parse models.yaml: {e}"));
        (s, Json(serde_json::to_value(j.0).unwrap_or_default()))
    })?;

    let data: Vec<ModelObject> = registry
        .models
        .iter()
        .filter(|m| m.enabled)
        .map(|m| ModelObject {
            id: m.name.clone(),
            object: "model",
            owned_by: "skycode",
            created: 0,
        })
        .collect();

    Ok(Json(ModelList {
        object: "list",
        data,
    }))
}
