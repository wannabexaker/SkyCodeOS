use serde::Deserialize;
use std::collections::HashMap;
use std::path::Path;
use thiserror::Error;

#[derive(Debug, Clone, PartialEq)]
pub enum PermissionSet {
    Default,
    Readonly,
    Sandbox,
}

#[derive(Debug, Clone)]
pub struct AgentProfile {
    pub name: String,
    pub model: String,
    pub temperature: f32,
    pub repeat_penalty: f32,
    pub max_tokens: u32,
    pub context_chunks: usize,
    pub permissions: PermissionSet,
    pub top_k: Option<u32>,
    pub top_p: Option<f32>,
    pub min_p: Option<f32>,
    pub typical_p: Option<f32>,
    pub repeat_last_n: Option<i32>,
    pub presence_penalty: Option<f32>,
    pub frequency_penalty: Option<f32>,
    pub dynatemp_range: Option<f32>,
    pub dynatemp_exponent: Option<f32>,
    pub dry_multiplier: Option<f32>,
    pub dry_base: Option<f32>,
    pub dry_allowed_length: Option<u32>,
    pub dry_penalty_last_n: Option<i32>,
    pub xtc_probability: Option<f32>,
    pub xtc_threshold: Option<f32>,
}

impl Default for AgentProfile {
    fn default() -> Self {
        Self {
            name: "precise".to_string(),
            model: "local-coder".to_string(),
            temperature: 0.1,
            repeat_penalty: 1.1,
            max_tokens: 2048,
            context_chunks: 5,
            permissions: PermissionSet::Default,
            top_k: None,
            top_p: None,
            min_p: None,
            typical_p: None,
            repeat_last_n: None,
            presence_penalty: None,
            frequency_penalty: None,
            dynatemp_range: None,
            dynatemp_exponent: None,
            dry_multiplier: None,
            dry_base: None,
            dry_allowed_length: None,
            dry_penalty_last_n: None,
            xtc_probability: None,
            xtc_threshold: None,
        }
    }
}

#[derive(Debug, Error)]
pub enum ProfileError {
    #[error("failed to read profiles.yaml: {0}")]
    Io(#[from] std::io::Error),
    #[error("failed to parse profiles.yaml: {0}")]
    Parse(#[from] serde_yaml::Error),
    #[error("profile '{0}' not found in profiles.yaml")]
    NotFound(String),
}

#[derive(Debug, Deserialize)]
struct ProfilesFile {
    profiles: HashMap<String, ProfileEntry>,
}

#[derive(Debug, Deserialize)]
struct ProfileEntry {
    model: String,
    temperature: f32,
    repeat_penalty: f32,
    max_tokens: u32,
    context_chunks: usize,
    permissions: String,
    #[serde(default)]
    top_k: Option<u32>,
    #[serde(default)]
    top_p: Option<f32>,
    #[serde(default)]
    min_p: Option<f32>,
    #[serde(default)]
    typical_p: Option<f32>,
    #[serde(default)]
    repeat_last_n: Option<i32>,
    #[serde(default)]
    presence_penalty: Option<f32>,
    #[serde(default)]
    frequency_penalty: Option<f32>,
    #[serde(default)]
    dynatemp_range: Option<f32>,
    #[serde(default)]
    dynatemp_exponent: Option<f32>,
    #[serde(default)]
    dry_multiplier: Option<f32>,
    #[serde(default)]
    dry_base: Option<f32>,
    #[serde(default)]
    dry_allowed_length: Option<u32>,
    #[serde(default)]
    dry_penalty_last_n: Option<i32>,
    #[serde(default)]
    xtc_probability: Option<f32>,
    #[serde(default)]
    xtc_threshold: Option<f32>,
}

pub fn load_profile(agents_root: &Path, name: &str) -> Result<AgentProfile, ProfileError> {
    let path = agents_root.join("profiles.yaml");

    if !path.exists() {
        return Ok(AgentProfile {
            name: name.to_string(),
            ..AgentProfile::default()
        });
    }

    let content = std::fs::read_to_string(&path)?;
    let file: ProfilesFile = serde_yaml::from_str(&content)?;
    let entry = file
        .profiles
        .get(name)
        .ok_or_else(|| ProfileError::NotFound(name.to_string()))?;

    Ok(AgentProfile {
        name: name.to_string(),
        model: entry.model.clone(),
        temperature: entry.temperature,
        repeat_penalty: entry.repeat_penalty,
        max_tokens: entry.max_tokens,
        context_chunks: entry.context_chunks,
        permissions: match entry.permissions.as_str() {
            "readonly" => PermissionSet::Readonly,
            "sandbox" => PermissionSet::Sandbox,
            _ => PermissionSet::Default,
        },
        top_k: entry.top_k,
        top_p: entry.top_p,
        min_p: entry.min_p,
        typical_p: entry.typical_p,
        repeat_last_n: entry.repeat_last_n,
        presence_penalty: entry.presence_penalty,
        frequency_penalty: entry.frequency_penalty,
        dynatemp_range: entry.dynatemp_range,
        dynatemp_exponent: entry.dynatemp_exponent,
        dry_multiplier: entry.dry_multiplier,
        dry_base: entry.dry_base,
        dry_allowed_length: entry.dry_allowed_length,
        dry_penalty_last_n: entry.dry_penalty_last_n,
        xtc_probability: entry.xtc_probability,
        xtc_threshold: entry.xtc_threshold,
    })
}
