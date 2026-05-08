use std::io::Write;
use std::path::Path;
use std::process::{Command, Stdio};
use std::time::{SystemTime, UNIX_EPOCH};

use rusqlite::Connection;
use thiserror::Error;

use skycode_core::approval::token::ApprovalToken;
use skycode_core::approval::validator::{validate_token, ValidatorError};
use skycode_core::db::events::{append_event, content_id, EventType, ToolEvent};

use super::diff::DiffProposal;

#[derive(Debug, Error)]
pub enum ApplyError {
    #[error("approval validation failed: {0}")]
    Validation(#[from] ValidatorError),
    #[error("failed to execute git apply: {0}")]
    Io(#[from] std::io::Error),
    #[error("git apply failed (status={status:?}): {stderr}")]
    GitApplyFailed { status: Option<i32>, stderr: String },
}

pub fn apply_diff(
    conn: &Connection,
    token: &ApprovalToken,
    expected_agent_id: &str,
    task_id: &str,
    repo_path: &Path,
    diff: &DiffProposal,
) -> Result<(), ApplyError> {
    validate_token(
        conn,
        token,
        &diff.id.to_string(),
        expected_agent_id,
        task_id,
    )?;

    let mut child = Command::new("git")
        .arg("-C")
        .arg(repo_path)
        .arg("apply")
        .arg("--whitespace=nowarn")
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .spawn()?;

    if let Some(stdin) = child.stdin.as_mut() {
        let mut patch = diff.diff_text.replace("\r\n", "\n").replace('\r', "\n");
        if !patch.ends_with('\n') {
            patch.push('\n');
        }
        stdin.write_all(patch.as_bytes())?;
    }

    let output = child.wait_with_output()?;
    if !output.status.success() {
        let status = output.status.code();
        let stderr = String::from_utf8_lossy(&output.stderr).to_string();
        record_apply_failed(conn, token, task_id, diff, status, &stderr);
        return Err(ApplyError::GitApplyFailed { status, stderr });
    }

    Ok(())
}

fn record_apply_failed(
    conn: &Connection,
    token: &ApprovalToken,
    task_id: &str,
    diff: &DiffProposal,
    status: Option<i32>,
    stderr: &str,
) {
    let Ok(created_at) = now_unix() else {
        eprintln!("warning: failed to record diff_apply_failed event: invalid system time");
        return;
    };

    let payload = format!(
        "diff_apply_failed:{}:{}:{:?}:{}",
        task_id, diff.id, status, stderr
    );
    let event = ToolEvent {
        id: content_id(payload.as_bytes()),
        task_id: task_id.to_string(),
        agent_id: token.agent_id.clone(),
        event_type: EventType::DiffApplyFailed,
        tool_name: Some("apply_diff".to_string()),
        inputs_hash: None,
        inputs_json: None,
        output_hash: None,
        output_json: Some(format!(
            "{{\"status\":{},\"stderr\":{}}}",
            status
                .map(|code| code.to_string())
                .unwrap_or_else(|| "null".to_string()),
            json_string(stderr)
        )),
        approval_token_id: Some(token.id.to_string()),
        diff_id: Some(diff.id.to_string()),
        profile_name: None,
        created_at,
    };

    if let Err(err) = append_event(conn, &event) {
        eprintln!("warning: failed to record diff_apply_failed event: {err}");
    }
}

fn json_string(value: &str) -> String {
    match serde_json::to_string(value) {
        Ok(encoded) => encoded,
        Err(_) => "\"\"".to_string(),
    }
}

fn now_unix() -> Result<i64, std::time::SystemTimeError> {
    let secs = SystemTime::now().duration_since(UNIX_EPOCH)?.as_secs();
    Ok(match i64::try_from(secs) {
        Ok(value) => value,
        Err(_) => i64::MAX,
    })
}
