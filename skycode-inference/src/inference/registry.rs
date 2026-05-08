use std::fs;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

use serde::{Deserialize, Serialize};
use thiserror::Error;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ModelRuntime {
    LocalGguf,
    OpenaiCompatible,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum SplitMode {
    #[default]
    Layer,
    Row,
    None,
}

impl SplitMode {
    pub fn as_flag(&self) -> &'static str {
        match self {
            Self::Layer => "layer",
            Self::Row => "row",
            Self::None => "none",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(untagged)]
pub enum VramBudget {
    Mb(u64),
    Auto(String),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ModelConfig {
    pub name: String,
    pub runtime: ModelRuntime,
    pub path: String,
    #[serde(default)]
    pub executable: Option<String>,
    pub ctx_size: usize,
    pub gpu_layers: usize,
    #[serde(default)]
    pub strengths: Vec<String>,
    pub enabled: bool,
    #[serde(default = "default_threads")]
    pub threads: usize,
    pub n_cpu_moe: Option<usize>,
    #[serde(default)]
    pub no_mmap: bool,
    #[serde(default)]
    pub mlock: bool,
    #[serde(default = "default_kv_offload")]
    pub kv_offload: bool,
    #[serde(default)]
    pub tensor_split: Vec<f64>,
    #[serde(default)]
    pub split_mode: SplitMode,
    #[serde(default)]
    pub vram_budget_mb: Option<VramBudget>,
    #[serde(default = "default_port")]
    pub port: u16,
}

fn default_threads() -> usize {
    8
}

fn default_kv_offload() -> bool {
    true
}

fn default_port() -> u16 {
    18080
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ModelRegistry {
    pub models: Vec<ModelConfig>,
}

#[derive(Debug)]
pub struct ModelRegistryWatcher {
    path: PathBuf,
    last_modified: Option<SystemTime>,
    registry: ModelRegistry,
}

#[derive(Debug, Error)]
pub enum ModelRegistryError {
    #[error("failed to read registry file: {0}")]
    Io(#[from] std::io::Error),
    #[error("failed to parse models.yaml: {0}")]
    Parse(#[from] serde_yaml::Error),
    #[error("remote adapter cannot be enabled in local registry: {0}")]
    RemoteAdapterEnabled(String),
    #[error("model not found in registry: {0}")]
    ModelNotFound(String),
    #[error("invalid tensor_split: {0}")]
    InvalidTensorSplit(String),
}

impl ModelRegistry {
    pub fn from_yaml(content: &str) -> Result<Self, ModelRegistryError> {
        let mut registry: ModelRegistry = serde_yaml::from_str(content)?;
        validate_registry(&mut registry)?;
        Ok(registry)
    }

    pub fn load_from_file(path: impl AsRef<Path>) -> Result<Self, ModelRegistryError> {
        let content = fs::read_to_string(path)?;
        Self::from_yaml(&content)
    }

    pub fn model(&self, name: &str) -> Option<&ModelConfig> {
        self.models.iter().find(|m| m.name == name)
    }
}

impl ModelRegistryWatcher {
    pub fn load(path: impl Into<PathBuf>) -> Result<Self, ModelRegistryError> {
        let path = path.into();
        let content = fs::read_to_string(&path)?;
        let registry = ModelRegistry::from_yaml(&content)?;
        let last_modified = fs::metadata(&path).and_then(|m| m.modified()).ok();

        Ok(Self {
            path,
            last_modified,
            registry,
        })
    }

    pub fn reload_if_changed(&mut self) -> Result<bool, ModelRegistryError> {
        let modified = fs::metadata(&self.path).and_then(|m| m.modified()).ok();
        let changed = modified != self.last_modified;

        if !changed {
            return Ok(false);
        }

        let content = fs::read_to_string(&self.path)?;
        self.registry = ModelRegistry::from_yaml(&content)?;
        self.last_modified = modified;
        Ok(true)
    }

    pub fn registry(&self) -> &ModelRegistry {
        &self.registry
    }

    pub fn model(&self, name: &str) -> Result<ModelConfig, ModelRegistryError> {
        self.registry
            .model(name)
            .cloned()
            .ok_or_else(|| ModelRegistryError::ModelNotFound(name.to_string()))
    }

    pub fn path(&self) -> &Path {
        &self.path
    }
}

fn validate_registry(registry: &mut ModelRegistry) -> Result<(), ModelRegistryError> {
    let has_remote = registry
        .models
        .iter()
        .any(|m| m.runtime == ModelRuntime::OpenaiCompatible);

    if !has_remote {
        registry.models.push(ModelConfig {
            name: "remote-adapter".to_string(),
            runtime: ModelRuntime::OpenaiCompatible,
            path: String::new(),
            executable: None,
            ctx_size: 0,
            gpu_layers: 0,
            strengths: Vec::new(),
            enabled: false,
            threads: default_threads(),
            n_cpu_moe: None,
            no_mmap: false,
            mlock: false,
            kv_offload: default_kv_offload(),
            tensor_split: Vec::new(),
            split_mode: SplitMode::Layer,
            vram_budget_mb: None,
            port: default_port(),
        });
    }

    for model in &mut registry.models {
        if !model.tensor_split.is_empty() {
            let sum: f64 = model.tensor_split.iter().copied().sum();
            if !(0.99..=1.01).contains(&sum) {
                return Err(ModelRegistryError::InvalidTensorSplit(format!(
                    "{} tensor_split sums to {}",
                    model.name, sum
                )));
            }
        }

        if model.runtime == ModelRuntime::OpenaiCompatible && model.enabled {
            return Err(ModelRegistryError::RemoteAdapterEnabled(model.name.clone()));
        }
    }

    Ok(())
}
