use serde_json::{Map, Value};
use thiserror::Error;

use super::response::SkyCoreResponse;

#[derive(Debug, Error)]
pub enum BoundaryError {
    #[error("raw provider payload must be a JSON object")]
    InvalidRoot,
    #[error("failed to deserialize sanitized SkyCore response: {0}")]
    Deserialize(#[from] serde_json::Error),
}

pub fn strip_provider_fields(raw: Value) -> Result<SkyCoreResponse, BoundaryError> {
    let sanitized = sanitize_response_value(raw)?;
    Ok(serde_json::from_value(sanitized)?)
}

fn sanitize_response_value(raw: Value) -> Result<Value, BoundaryError> {
    let Value::Object(mut obj) = raw else {
        return Err(BoundaryError::InvalidRoot);
    };

    let mut out = Map::new();

    if let Some(v) = obj.remove("task_id") {
        out.insert("task_id".to_string(), v);
    }
    if let Some(v) = obj.remove("skycore_version") {
        out.insert("skycore_version".to_string(), v);
    } else {
        out.insert(
            "skycore_version".to_string(),
            Value::String("0.1".to_string()),
        );
    }
    if let Some(v) = obj.remove("status") {
        out.insert("status".to_string(), v);
    }
    if let Some(v) = obj.remove("summary") {
        out.insert("summary".to_string(), v);
    }
    if let Some(v) = obj.remove("requires_approval") {
        out.insert("requires_approval".to_string(), v);
    }
    if let Some(v) = obj.remove("error") {
        out.insert("error".to_string(), v);
    } else {
        out.insert("error".to_string(), Value::Null);
    }

    if let Some(artifacts) = obj.remove("artifacts") {
        let sanitized_artifacts = sanitize_artifacts(artifacts);
        out.insert("artifacts".to_string(), sanitized_artifacts);
    } else {
        out.insert("artifacts".to_string(), Value::Array(Vec::new()));
    }

    if let Some(tool_calls) = obj.remove("tool_calls_requested") {
        out.insert(
            "tool_calls_requested".to_string(),
            sanitize_tool_calls(tool_calls),
        );
    } else {
        out.insert("tool_calls_requested".to_string(), Value::Array(Vec::new()));
    }

    Ok(Value::Object(out))
}

fn sanitize_artifacts(value: Value) -> Value {
    let Value::Array(items) = value else {
        return Value::Array(Vec::new());
    };

    let mut out = Vec::with_capacity(items.len());
    for item in items {
        let Value::Object(mut obj) = item else {
            continue;
        };

        let mut clean = Map::new();
        if let Some(v) = obj.remove("kind") {
            clean.insert("kind".to_string(), v);
        }
        if let Some(v) = obj.remove("id") {
            clean.insert("id".to_string(), v);
        }
        if let Some(v) = obj.remove("patch_unified") {
            clean.insert("patch_unified".to_string(), v);
        }
        if let Some(v) = obj.remove("new_content") {
            clean.insert("new_content".to_string(), v);
        }
        if let Some(v) = obj.remove("content") {
            clean.insert("content".to_string(), v);
        }
        if let Some(v) = obj.remove("affected_files") {
            clean.insert("affected_files".to_string(), v);
        }

        out.push(Value::Object(clean));
    }

    Value::Array(out)
}

fn sanitize_tool_calls(value: Value) -> Value {
    let Value::Array(items) = value else {
        return Value::Array(Vec::new());
    };

    let mut out = Vec::with_capacity(items.len());
    for item in items {
        let Value::Object(mut obj) = item else {
            continue;
        };

        let mut clean = Map::new();
        if let Some(v) = obj.remove("tool") {
            clean.insert("tool".to_string(), v);
        }
        if let Some(v) = obj.remove("inputs") {
            clean.insert("inputs".to_string(), v);
        }
        out.push(Value::Object(clean));
    }

    Value::Array(out)
}

#[cfg(test)]
mod tests {
    use super::{sanitize_response_value, strip_provider_fields};
    use serde_json::json;

    #[test]
    fn strips_openai_provider_fields() {
        let raw = json!({
            "skycore_version": "0.1",
            "task_id": "task-1",
            "status": "ok",
            "summary": "done",
            "tool_calls_requested": [],
            "requires_approval": true,
            "error": null,
            "artifacts": [
                {
                    "kind": "diff",
                    "id": "patch-1",
                    "patch_unified": "--- a\n+++ b\n",
                    "affected_files": ["src/lib.rs"],
                    "provider_blob": {"x": 1}
                }
            ],
            "choices": [{"message": {"content": "provider response"}}],
            "usage": {"prompt_tokens": 12},
            "model": "gpt-4o-mini"
        });

        let sanitized = sanitize_response_value(raw).expect("sanitize response");
        let object = sanitized.as_object().expect("sanitized object");

        assert!(!object.contains_key("choices"));
        assert!(!object.contains_key("usage"));
        assert!(!object.contains_key("model"));

        let parsed = strip_provider_fields(sanitized).expect("deserialize skycore response");
        assert_eq!(parsed.task_id, "task-1");
        assert!(parsed.requires_approval);
    }
}
