pub mod context;
pub mod loader;
pub mod registry;

pub use context::{fit_context, ContextSlot, ContextWindow};
pub use loader::{
    build_llama_server_argv, call_model, is_mlock_warning_line, launch_model, launch_server,
    InferenceError, ModelHandle, ModelLaunchOptions, ModelLoadError,
};
pub use registry::{
    ModelConfig, ModelRegistry, ModelRegistryError, ModelRegistryWatcher, ModelRuntime, SplitMode,
    VramBudget,
};
