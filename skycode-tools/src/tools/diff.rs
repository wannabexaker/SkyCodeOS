use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

use thiserror::Error;
use uuid::Uuid;

#[derive(Debug, Clone)]
pub struct DiffProposal {
    pub id: Uuid,
    pub project_id: String,
    pub diff_text: String,
    pub file_path: String,
    pub created_at: i64,
}

#[derive(Debug, Error)]
pub enum DiffError {
    #[error("path is not valid UTF-8")]
    InvalidPath,
    #[error("failed to read system time")]
    InvalidSystemTime,
}

pub fn create_diff(
    project_id: impl Into<String>,
    file_path: &Path,
    before: &str,
    after: &str,
) -> Result<DiffProposal, DiffError> {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|_| DiffError::InvalidSystemTime)?
        .as_secs();

    let created_at = i64::try_from(now).map_err(|_| DiffError::InvalidSystemTime)?;
    let file_path_str = file_path
        .to_str()
        .ok_or(DiffError::InvalidPath)?
        .to_string();

    let diff_text = format!(
        "--- a/{path}\n+++ b/{path}\n@@ -1 +1 @@\n-{before}\n+{after}\n",
        path = file_path_str,
        before = normalize_for_unified_diff(before),
        after = normalize_for_unified_diff(after),
    );

    Ok(DiffProposal {
        id: Uuid::new_v4(),
        project_id: project_id.into(),
        diff_text,
        file_path: file_path_str,
        created_at,
    })
}

fn normalize_for_unified_diff(input: &str) -> String {
    input.replace('\r', "").replace('\n', "\\n")
}
