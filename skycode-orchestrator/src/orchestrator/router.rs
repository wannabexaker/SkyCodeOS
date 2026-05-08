use rusqlite::Connection;
use thiserror::Error;

use skycode_core::db::events::{append_event, content_id, EventType, EventsError, ToolEvent};
use skycode_inference::inference::registry::{ModelConfig, ModelRegistry, ModelRuntime};

/// Broad classes of coding tasks used to select the right model profile.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TaskClass {
    CodeEdit,
    ShortAnswer,
    Refactor,
    Plan,
}

#[derive(Debug, Error)]
pub enum RouterError {
    #[error("no suitable local model found for task class {0:?}; remote adapter is disabled")]
    NoLocalModel(TaskClass),
    #[error("database error: {0}")]
    Database(#[from] rusqlite::Error),
    #[error("events error: {0}")]
    Events(#[from] EventsError),
    #[error("invalid system time")]
    InvalidSystemTime,
}

/// Classify a goal string into a TaskClass using keyword heuristics only.
/// No secondary model call is made.
pub fn classify_task(goal: &str) -> TaskClass {
    let lower = goal.to_lowercase();

    if lower.contains("rename")
        || lower.contains("move")
        || lower.contains("extract")
        || lower.contains("refactor")
    {
        return TaskClass::Refactor;
    }

    if lower.contains("fix")
        || lower.contains("bug")
        || lower.contains("error")
        || lower.contains("crash")
    {
        return TaskClass::CodeEdit;
    }

    if lower.contains("explain")
        || lower.contains("what")
        || lower.contains("why")
        || lower.contains("how")
    {
        return TaskClass::ShortAnswer;
    }

    if lower.contains("plan") || lower.contains("design") || lower.contains("architect") {
        return TaskClass::Plan;
    }

    TaskClass::CodeEdit
}

/// Map a TaskClass to the best available local model in the registry.
///
/// Selection order:
///   ShortAnswer  → local-coder-fast  → local-coder     → any local → Err
///   everything   → local-coder       → local-coder-fast → any local → Err
///
/// Remote adapter is NEVER selected — returning Err instead is intentional.
pub fn map_to_model<'a>(
    class: TaskClass,
    registry: &'a ModelRegistry,
) -> Result<&'a ModelConfig, RouterError> {
    let preferred = match class {
        TaskClass::ShortAnswer => "local-coder-fast",
        _ => "local-coder",
    };

    if let Some(m) = find_local(registry, preferred) {
        return Ok(m);
    }

    // First fallback: swap preferred/secondary
    let secondary = if preferred == "local-coder-fast" {
        "local-coder"
    } else {
        "local-coder-fast"
    };

    if let Some(m) = find_local(registry, secondary) {
        return Ok(m);
    }

    // Last resort: any enabled local_gguf model
    if let Some(m) = registry
        .models
        .iter()
        .find(|m| m.runtime == ModelRuntime::LocalGguf && m.enabled)
    {
        return Ok(m);
    }

    Err(RouterError::NoLocalModel(class))
}

/// Write a `model_invoked` telemetry event to `tool_events`.
pub fn record_model_selection(
    conn: &Connection,
    task_id: &str,
    model_name: &str,
    profile_name: &str,
) -> Result<(), RouterError> {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_err(|_| RouterError::InvalidSystemTime)?
        .as_secs();
    let now_i64 = i64::try_from(now).map_err(|_| RouterError::InvalidSystemTime)?;

    let payload = format!("model_invoked:{}:{}", task_id, model_name);
    let event = ToolEvent {
        id: content_id(payload.as_bytes()),
        task_id: task_id.to_string(),
        agent_id: "coder-primary".to_string(),
        event_type: EventType::ModelInvoked,
        tool_name: Some(model_name.to_string()),
        inputs_hash: None,
        inputs_json: None,
        output_hash: None,
        output_json: None,
        approval_token_id: None,
        diff_id: None,
        profile_name: Some(profile_name.to_string()),
        created_at: now_i64,
    };

    append_event(conn, &event)?;
    Ok(())
}

fn find_local<'a>(registry: &'a ModelRegistry, name: &str) -> Option<&'a ModelConfig> {
    registry
        .models
        .iter()
        .find(|m| m.name == name && m.runtime == ModelRuntime::LocalGguf && m.enabled)
}
