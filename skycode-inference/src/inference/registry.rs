use std::fmt;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

use serde::de::{SeqAccess, Visitor};
use serde::ser::SerializeSeq;
use serde::{Deserialize, Deserializer, Serialize, Serializer};
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

#[derive(Debug, Clone, PartialEq)]
pub enum GpuLayerSpec {
    Fixed(usize),
    Auto,
}

impl Default for GpuLayerSpec {
    fn default() -> Self {
        Self::Fixed(0)
    }
}

impl<'de> Deserialize<'de> for GpuLayerSpec {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        deserializer.deserialize_any(GpuLayerSpecVisitor)
    }
}

impl Serialize for GpuLayerSpec {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        match self {
            Self::Fixed(value) => serializer.serialize_u64(*value as u64),
            Self::Auto => serializer.serialize_str("auto"),
        }
    }
}

struct GpuLayerSpecVisitor;

impl<'de> Visitor<'de> for GpuLayerSpecVisitor {
    type Value = GpuLayerSpec;

    fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("an integer (fixed layer count) or \"auto\"")
    }

    fn visit_u64<E>(self, value: u64) -> Result<Self::Value, E>
    where
        E: serde::de::Error,
    {
        Ok(GpuLayerSpec::Fixed(value as usize))
    }

    fn visit_i64<E>(self, value: i64) -> Result<Self::Value, E>
    where
        E: serde::de::Error,
    {
        if value < 0 {
            return Err(E::custom("gpu_layers must be non-negative"));
        }
        Ok(GpuLayerSpec::Fixed(value as usize))
    }

    fn visit_str<E>(self, value: &str) -> Result<Self::Value, E>
    where
        E: serde::de::Error,
    {
        if value == "auto" {
            Ok(GpuLayerSpec::Auto)
        } else {
            Err(E::custom("expected gpu_layers to be \"auto\""))
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum TensorSplitSpec {
    Fixed(Vec<f64>),
    Auto,
}

impl Default for TensorSplitSpec {
    fn default() -> Self {
        Self::Fixed(Vec::new())
    }
}

impl<'de> Deserialize<'de> for TensorSplitSpec {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        deserializer.deserialize_any(TensorSplitSpecVisitor)
    }
}

impl Serialize for TensorSplitSpec {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        match self {
            Self::Fixed(values) => {
                let mut seq = serializer.serialize_seq(Some(values.len()))?;
                for value in values {
                    seq.serialize_element(value)?;
                }
                seq.end()
            }
            Self::Auto => serializer.serialize_str("auto"),
        }
    }
}

struct TensorSplitSpecVisitor;

impl<'de> Visitor<'de> for TensorSplitSpecVisitor {
    type Value = TensorSplitSpec;

    fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("a list of f64 ratios or \"auto\"")
    }

    fn visit_seq<A>(self, mut seq: A) -> Result<Self::Value, A::Error>
    where
        A: SeqAccess<'de>,
    {
        let mut values = Vec::new();
        while let Some(value) = seq.next_element::<f64>()? {
            values.push(value);
        }
        Ok(TensorSplitSpec::Fixed(values))
    }

    fn visit_str<E>(self, value: &str) -> Result<Self::Value, E>
    where
        E: serde::de::Error,
    {
        if value == "auto" {
            Ok(TensorSplitSpec::Auto)
        } else {
            Err(E::custom("expected tensor_split to be \"auto\""))
        }
    }
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
    #[serde(default)]
    pub gpu_layers: GpuLayerSpec,
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
    pub tensor_split: TensorSplitSpec,
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
            gpu_layers: GpuLayerSpec::Fixed(0),
            strengths: Vec::new(),
            enabled: false,
            threads: default_threads(),
            n_cpu_moe: None,
            no_mmap: false,
            mlock: false,
            kv_offload: default_kv_offload(),
            tensor_split: TensorSplitSpec::Fixed(Vec::new()),
            split_mode: SplitMode::Layer,
            vram_budget_mb: None,
            port: default_port(),
        });
    }

    for model in &mut registry.models {
        if let TensorSplitSpec::Fixed(values) = &model.tensor_split {
            if !values.is_empty() {
                let sum: f64 = values.iter().copied().sum();
                if !(0.99..=1.01).contains(&sum) {
                    return Err(ModelRegistryError::InvalidTensorSplit(format!(
                        "{} tensor_split sums to {}",
                        model.name, sum
                    )));
                }
            }
        }

        if model.runtime == ModelRuntime::OpenaiCompatible && model.enabled {
            return Err(ModelRegistryError::RemoteAdapterEnabled(model.name.clone()));
        }
    }

    Ok(())
}
