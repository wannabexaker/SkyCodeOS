use std::path::Path;
use std::process::Command;

use thiserror::Error;

#[derive(Debug, Error)]
pub enum RollbackError {
    #[error("failed to execute git checkout: {0}")]
    Io(#[from] std::io::Error),
    #[error("git checkout failed (status={status:?}): {stderr}")]
    GitCheckoutFailed { status: Option<i32>, stderr: String },
}

pub fn rollback(repo_path: &Path, pre_apply_git_ref: &str) -> Result<(), RollbackError> {
    let output = Command::new("git")
        .arg("-C")
        .arg(repo_path)
        .arg("checkout")
        .arg(pre_apply_git_ref)
        .arg("--")
        .arg(".")
        .output()?;

    if !output.status.success() {
        return Err(RollbackError::GitCheckoutFailed {
            status: output.status.code(),
            stderr: String::from_utf8_lossy(&output.stderr).to_string(),
        });
    }

    Ok(())
}
