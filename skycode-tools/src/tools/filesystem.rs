use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::process::Command;

use thiserror::Error;

#[derive(Debug, Error)]
pub enum FilesystemError {
    #[error("io error: {0}")]
    Io(#[from] io::Error),
    #[error("command failed: {command} (status={status:?}) stderr={stderr}")]
    CommandFailed {
        command: String,
        status: Option<i32>,
        stderr: String,
    },
}

pub fn read_file(path: &Path) -> Result<String, FilesystemError> {
    Ok(fs::read_to_string(path)?)
}

pub fn list_dir(path: &Path) -> Result<Vec<PathBuf>, FilesystemError> {
    let mut entries = Vec::new();
    for entry in fs::read_dir(path)? {
        let entry = entry?;
        entries.push(entry.path());
    }
    entries.sort();
    Ok(entries)
}

pub fn search_project(root: &Path, query: &str) -> Result<Vec<PathBuf>, FilesystemError> {
    let mut matches = Vec::new();
    visit_dir(root, query, &mut matches)?;
    matches.sort();
    Ok(matches)
}

fn visit_dir(path: &Path, query: &str, matches: &mut Vec<PathBuf>) -> Result<(), FilesystemError> {
    for entry in fs::read_dir(path)? {
        let entry = entry?;
        let entry_path = entry.path();

        if entry.file_type()?.is_dir() {
            visit_dir(&entry_path, query, matches)?;
            continue;
        }

        if !entry.file_type()?.is_file() {
            continue;
        }

        if let Ok(contents) = fs::read_to_string(&entry_path) {
            if contents.contains(query) {
                matches.push(entry_path);
            }
        }
    }

    Ok(())
}

pub fn git_status(repo_path: &Path) -> Result<String, FilesystemError> {
    let output = Command::new("git")
        .arg("-C")
        .arg(repo_path)
        .arg("status")
        .arg("--short")
        .arg("--branch")
        .output()?;

    if !output.status.success() {
        return Err(FilesystemError::CommandFailed {
            command: "git status --short --branch".to_string(),
            status: output.status.code(),
            stderr: String::from_utf8_lossy(&output.stderr).to_string(),
        });
    }

    Ok(String::from_utf8_lossy(&output.stdout).to_string())
}
