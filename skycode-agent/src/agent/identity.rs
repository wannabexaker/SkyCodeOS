use std::fs;
use std::path::{Path, PathBuf};

use serde::Deserialize;
use thiserror::Error;

#[derive(Debug, Clone)]
pub struct AgentIdentity {
    pub id: String,
    pub name: String,
    pub role: String,
    pub core_values: Vec<String>,
    pub communication_style: String,
    pub error_handling: String,
    pub planning_depth: String,
    pub risk_tolerance: String,
    pub must_never: Vec<String>,
    pub must_always: Vec<String>,
    pub approval_required_for: Vec<String>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct SoulYaml {
    id: String,
    name: String,
    role: String,
    core_values: Vec<String>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct HeartYaml {
    communication_style: String,
    error_handling: String,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct MindYaml {
    planning_depth: String,
    risk_tolerance: String,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct DoctrineYaml {
    must_never: Vec<String>,
    must_always: Vec<String>,
    approval_required_for: Vec<String>,
}

#[derive(Debug, Error)]
pub enum IdentityError {
    #[error("failed to read identity file: {0}")]
    Io(#[from] std::io::Error),
    #[error("failed to parse identity yaml: {0}")]
    Parse(#[from] serde_yaml::Error),
    #[error("agents root not found: {0}")]
    AgentsRootMissing(String),
    #[error("exactly one agent must be loaded at startup, found {0}")]
    InvalidAgentCount(usize),
}

pub fn load_coder_primary_identity(agents_root: &Path) -> Result<AgentIdentity, IdentityError> {
    assert_single_agent(agents_root)?;

    let core_dir = agents_root.join("coder-primary").join("core");

    let soul: SoulYaml = parse_yaml(core_dir.join("soul.yaml"))?;
    let heart: HeartYaml = parse_yaml(core_dir.join("heart.yaml"))?;
    let mind: MindYaml = parse_yaml(core_dir.join("mind.yaml"))?;
    let doctrine: DoctrineYaml = parse_yaml(core_dir.join("doctrine.yaml"))?;

    Ok(AgentIdentity {
        id: soul.id,
        name: soul.name,
        role: soul.role,
        core_values: soul.core_values,
        communication_style: heart.communication_style,
        error_handling: heart.error_handling,
        planning_depth: mind.planning_depth,
        risk_tolerance: mind.risk_tolerance,
        must_never: doctrine.must_never,
        must_always: doctrine.must_always,
        approval_required_for: doctrine.approval_required_for,
    })
}

fn parse_yaml<T: for<'de> Deserialize<'de>>(path: PathBuf) -> Result<T, IdentityError> {
    let content = fs::read_to_string(path)?;
    Ok(serde_yaml::from_str(&content)?)
}

fn assert_single_agent(agents_root: &Path) -> Result<(), IdentityError> {
    if !agents_root.exists() {
        return Err(IdentityError::AgentsRootMissing(
            agents_root.display().to_string(),
        ));
    }

    let mut count = 0usize;
    for entry in fs::read_dir(agents_root)? {
        let entry = entry?;
        if entry.file_type()?.is_dir() {
            let name = entry.file_name();
            let name = name.to_string_lossy();
            if !matches!(name.as_ref(), "models" | "grammars") {
                count += 1;
            }
        }
    }

    assert_eq!(count, 1, "exactly one agent must be loaded at startup");

    Ok(())
}
