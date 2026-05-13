use std::io::Write;
use std::path::Path;
use std::process::{Command, Stdio};
use std::time::{SystemTime, UNIX_EPOCH};

use rusqlite::Connection;
use thiserror::Error;

use skycode_core::approval::token::ApprovalToken;
use skycode_core::approval::validator::{validate_token, ValidatorError};
use skycode_core::db::diff_sets::{get_diff_set_members, DiffSetMember};
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

#[derive(Debug, Error)]
pub enum DiffSetApplyError {
    #[error("diff set has no members: {0}")]
    EmptySet(String),
    #[error("no token provided for diff {0}")]
    MissingToken(String),
    #[error("precheck failed for diff {diff_id}: {stderr}")]
    PrecheckFailed { diff_id: String, stderr: String },
    #[error("approval validation failed for diff {diff_id}: {source}")]
    Validation {
        diff_id: String,
        source: ValidatorError,
    },
    #[error("apply failed on diff {diff_id} ({applied_count} of {total} applied): {stderr}")]
    MidFlightFailure {
        diff_id: String,
        applied_count: usize,
        total: usize,
        stderr: String,
    },
    #[error("git stash failed: {0}")]
    GitStash(String),
    #[error("database error: {0}")]
    Database(#[from] rusqlite::Error),
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
}

pub fn apply_diff(
    conn: &Connection,
    token: &ApprovalToken,
    expected_agent_id: &str,
    task_id: &str,
    repo_path: &Path,
    project_id: &str,
    diff: &DiffProposal,
) -> Result<(), ApplyError> {
    validate_token(
        conn,
        token,
        project_id,
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
        let patch = normalize_patch(&diff.diff_text);
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

pub fn apply_diff_set(
    conn: &Connection,
    set_id: &str,
    tokens: &[ApprovalToken],
    expected_agent_id: &str,
    task_id: &str,
    repo_path: &Path,
    project_id: &str,
    diffs: &[DiffProposal],
) -> Result<(), DiffSetApplyError> {
    let members = get_diff_set_members(conn, set_id).map_err(|err| match err {
        skycode_core::db::diff_sets::DiffSetError::Database(db_err) => {
            DiffSetApplyError::Database(db_err)
        }
        skycode_core::db::diff_sets::DiffSetError::MembershipFrozen(_) => {
            DiffSetApplyError::EmptySet(set_id.to_string())
        }
    })?;
    if members.is_empty() {
        return Err(DiffSetApplyError::EmptySet(set_id.to_string()));
    }

    // Phase 1 — precheck (zero side effects).
    for member in &members {
        let diff = find_member_diff(diffs, member, set_id)?;
        let output = run_git_apply(repo_path, &diff.diff_text, true)?;
        if !output.status.success() {
            return Err(DiffSetApplyError::PrecheckFailed {
                diff_id: member.diff_id.clone(),
                stderr: String::from_utf8_lossy(&output.stderr).to_string(),
            });
        }
    }

    // Phase 2 — token validation (zero side effects).
    for member in &members {
        let token = tokens
            .iter()
            .find(|token| token.diff_id == member.diff_id)
            .ok_or_else(|| DiffSetApplyError::MissingToken(member.diff_id.clone()))?;

        validate_token(
            conn,
            token,
            project_id,
            &member.diff_id,
            expected_agent_id,
            task_id,
        )
        .map_err(|source| DiffSetApplyError::Validation {
            diff_id: member.diff_id.clone(),
            source,
        })?;
    }

    // Phase 3 — git stash.
    let stash_output = Command::new("git")
        .arg("-C")
        .arg(repo_path)
        .arg("stash")
        .arg("push")
        .arg("-m")
        .arg(format!("scos-apply-{set_id}"))
        .output()?;

    // git stash exits 0 in two situations:
    //   (a) changes were stashed  → stdout does NOT contain "No local changes"
    //   (b) working tree was clean → stdout contains "No local changes to save"
    // We must distinguish them so Phase 5 doesn't attempt `stash pop` on an empty stash.
    let stashed = if stash_output.status.success() {
        let stdout = String::from_utf8_lossy(&stash_output.stdout);
        !stdout.contains("No local changes to save") && !stdout.contains("Nothing to save")
    } else {
        let stderr = String::from_utf8_lossy(&stash_output.stderr).to_string();
        if stderr.contains("nothing to stash") {
            false
        } else {
            return Err(DiffSetApplyError::GitStash(stderr));
        }
    };

    // Phase 4 — apply loop.
    let mut applied_count = 0usize;
    for member in &members {
        let diff = find_member_diff(diffs, member, set_id)?;
        let output = run_git_apply(repo_path, &diff.diff_text, false)?;

        if output.status.success() {
            applied_count += 1;
            record_diff_applied_best_effort(conn, tokens, task_id, member, diff);
            continue;
        }

        // Mid-flight failure: reset working tree to pre-apply state, then restore stash
        let _ = Command::new("git")
            .arg("-C")
            .arg(repo_path)
            .arg("reset")
            .arg("--hard")
            .arg("HEAD")
            .output();

        if stashed {
            let _ = Command::new("git")
                .arg("-C")
                .arg(repo_path)
                .arg("stash")
                .arg("pop")
                .arg("--index")
                .output();
        }

        return Err(DiffSetApplyError::MidFlightFailure {
            diff_id: member.diff_id.clone(),
            applied_count,
            total: members.len(),
            stderr: String::from_utf8_lossy(&output.stderr).to_string(),
        });
    }

    // Phase 5 — restore stash on success path.
    if stashed {
        let pop_output = Command::new("git")
            .arg("-C")
            .arg(repo_path)
            .arg("stash")
            .arg("pop")
            .arg("--index")
            .output()?;

        if !pop_output.status.success() {
            return Err(DiffSetApplyError::GitStash(
                String::from_utf8_lossy(&pop_output.stderr).to_string(),
            ));
        }
    }

    Ok(())
}

fn find_member_diff<'a>(
    diffs: &'a [DiffProposal],
    member: &DiffSetMember,
    set_id: &str,
) -> Result<&'a DiffProposal, DiffSetApplyError> {
    diffs
        .iter()
        .find(|diff| diff.id.to_string() == member.diff_id)
        .ok_or_else(|| DiffSetApplyError::EmptySet(set_id.to_string()))
}

fn run_git_apply(
    repo_path: &Path,
    diff_text: &str,
    check_only: bool,
) -> Result<std::process::Output, std::io::Error> {
    let mut command = Command::new("git");
    command
        .arg("-C")
        .arg(repo_path)
        .arg("apply")
        .arg("--whitespace=nowarn");

    if check_only {
        command.arg("--check");
    }

    let mut child = command
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .spawn()?;

    if let Some(stdin) = child.stdin.as_mut() {
        let patch = normalize_patch(diff_text);
        stdin.write_all(patch.as_bytes())?;
    }

    child.wait_with_output()
}

fn normalize_patch(diff_text: &str) -> String {
    let mut patch = diff_text.replace("\r\n", "\n").replace('\r', "\n");
    if !patch.ends_with('\n') {
        patch.push('\n');
    }
    patch
}

fn record_diff_applied_best_effort(
    conn: &Connection,
    tokens: &[ApprovalToken],
    task_id: &str,
    member: &DiffSetMember,
    diff: &DiffProposal,
) {
    let created_at = match now_unix() {
        Ok(value) => value,
        Err(_) => {
            eprintln!("warning: failed to record diff_applied event: invalid system time");
            return;
        }
    };

    let token = tokens.iter().find(|token| token.diff_id == member.diff_id);
    let payload = format!("diff_applied:{}:{}", task_id, member.diff_id);
    let event = ToolEvent {
        id: content_id(payload.as_bytes()),
        task_id: task_id.to_string(),
        agent_id: token
            .map(|token| token.agent_id.clone())
            .unwrap_or_else(|| "unknown-agent".to_string()),
        event_type: EventType::DiffApplied,
        tool_name: Some("apply_diff_set".to_string()),
        inputs_hash: None,
        inputs_json: None,
        output_hash: None,
        output_json: None,
        approval_token_id: token.map(|token| token.id.to_string()),
        diff_id: Some(diff.id.to_string()),
        profile_name: None,
        created_at,
    };

    if let Err(err) = append_event(conn, &event) {
        eprintln!("warning: failed to record diff_applied event: {err}");
    }
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
