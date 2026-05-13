use axum::http::StatusCode;
use serde::Serialize;

#[derive(Debug, Serialize)]
pub struct ApiError {
    pub error: ApiErrorBody,
}

#[derive(Debug, Serialize)]
pub struct ApiErrorBody {
    pub message: String,
    #[serde(rename = "type")]
    pub error_type: String,
    pub code: String,
}

impl ApiError {
    pub fn unauthorized() -> (StatusCode, axum::Json<Self>) {
        (
            StatusCode::UNAUTHORIZED,
            axum::Json(Self {
                error: ApiErrorBody {
                    message: "Invalid or missing API key".to_string(),
                    error_type: "invalid_request_error".to_string(),
                    code: "invalid_api_key".to_string(),
                },
            }),
        )
    }

    pub fn internal(msg: impl Into<String>) -> (StatusCode, axum::Json<Self>) {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            axum::Json(Self {
                error: ApiErrorBody {
                    message: msg.into(),
                    error_type: "api_error".to_string(),
                    code: "internal_error".to_string(),
                },
            }),
        )
    }

    pub fn not_found(msg: impl Into<String>) -> (StatusCode, axum::Json<Self>) {
        (
            StatusCode::NOT_FOUND,
            axum::Json(Self {
                error: ApiErrorBody {
                    message: msg.into(),
                    error_type: "invalid_request_error".to_string(),
                    code: "not_found".to_string(),
                },
            }),
        )
    }
}
