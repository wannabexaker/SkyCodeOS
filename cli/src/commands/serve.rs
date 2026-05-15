use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use clap::Args;
use rusqlite::Connection;

use skycode_api::state::AppState;
use skycode_inference::inference::{
    launch_server, resolve_gpu_layers, resolve_tensor_split, ModelConfig, ModelHandle,
    ModelLaunchOptions, ModelRegistry, ModelRuntime,
};
use skycode_orchestrator::db::migrations::run_migrations;
use skycode_orchestrator::orchestrator::{map_to_model, TaskClass};

#[derive(Debug, Args)]
pub struct ServeArgs {
    /// Host to bind (default: 0.0.0.0 - LAN accessible).
    #[arg(long, default_value = "0.0.0.0")]
    pub host: String,

    /// Port to listen on (default: 11434, same as Ollama).
    #[arg(long, default_value_t = 11434)]
    pub port: u16,

    /// Do not spawn llama-server; expect an external upstream at the configured model port.
    #[arg(long)]
    pub no_spawn: bool,
}

pub fn run(args: &ServeArgs) -> Result<(), Box<dyn std::error::Error>> {
    let project_root = std::env::current_dir()?;
    let api_key = skycode_api::key::load_or_create(&project_root)?;

    println!("API key: {api_key}");
    println!("Store this key in SKYCODE_API_KEY on client machines.");

    // Open database and run migrations
    let db_path = project_root.join("skycode.db");
    let conn = Connection::open(&db_path)?;
    let migrations_dir = project_root.join("memory").join("migrations");
    if migrations_dir.exists() {
        run_migrations(&conn, &migrations_dir)?;
    }
    let conn = Arc::new(Mutex::new(conn));

    let state = AppState {
        api_key: Arc::new(api_key),
        models_yaml_path: project_root.join("agents").join("models.yaml"),
        db_path,
        project_root,
        http_client: reqwest::Client::new(),
        conn,
    };

    let _llama_server = prepare_llama_server(&state.project_root, args.no_spawn)?;

    let rt = tokio::runtime::Runtime::new()?;
    rt.block_on(skycode_api::server::run(state, &args.host, args.port))?;
    Ok(())
}

#[allow(dead_code)]
pub struct SpawnedLlama<H> {
    pub handle: H,
    pub pid: u32,
    pub port: u16,
}

pub fn prepare_llama_server(
    project_root: &Path,
    no_spawn: bool,
) -> Result<Option<SpawnedLlama<ModelHandle>>, Box<dyn std::error::Error>> {
    prepare_llama_server_with_launcher(project_root, no_spawn, |launch| {
        let handle = launch_server(launch)?;
        let pid = handle.process.id();
        Ok((handle, pid))
    })
}

pub fn prepare_llama_server_with_launcher<H, F>(
    project_root: &Path,
    no_spawn: bool,
    launcher: F,
) -> Result<Option<SpawnedLlama<H>>, Box<dyn std::error::Error>>
where
    F: FnOnce(&ModelLaunchOptions) -> Result<(H, u32), Box<dyn std::error::Error>>,
{
    let model = select_default_serve_model(project_root)?;
    let launch = launch_options_for_model(&model);

    if no_spawn {
        println!("llama-server: external mode (--no-spawn), expecting an upstream at");
        println!("              http://127.0.0.1:{}", launch.port);
        return Ok(None);
    }

    let (handle, pid) = launcher(&launch)?;
    println!(
        "llama-server child spawned on 127.0.0.1:{} (pid {})",
        launch.port, pid
    );

    Ok(Some(SpawnedLlama {
        handle,
        pid,
        port: launch.port,
    }))
}

pub fn select_default_serve_model(
    project_root: &Path,
) -> Result<ModelConfig, Box<dyn std::error::Error>> {
    let models_path = project_root.join("agents").join("models.yaml");
    let registry = ModelRegistry::load_from_file(&models_path)?;
    let model = match map_to_model(TaskClass::CodeEdit, &registry) {
        Ok(model) => model.clone(),
        Err(_) => registry
            .models
            .iter()
            .find(|model| model.runtime == ModelRuntime::LocalGguf && model.enabled)
            .cloned()
            .ok_or("no enabled local_gguf model found in agents/models.yaml")?,
    };

    if model.runtime != ModelRuntime::LocalGguf || !model.enabled {
        return Err("selected serve model must be an enabled local_gguf entry".into());
    }

    Ok(model)
}

pub fn launch_options_for_model(model: &ModelConfig) -> ModelLaunchOptions {
    ModelLaunchOptions {
        executable: model.executable.clone(),
        model_path: PathBuf::from(&model.path),
        ctx_size: model.ctx_size,
        threads: model.threads,
        n_gpu_layers: resolve_gpu_layers(
            &model.gpu_layers,
            Path::new(&model.path),
            &model.vram_budget_mb,
        ),
        n_cpu_moe: model.n_cpu_moe,
        prompt: None,
        temp: 0.1,
        repeat_penalty: 1.1,
        max_tokens: 2048,
        no_mmap: model.no_mmap,
        mlock: model.mlock,
        port: model.port,
        kv_offload: model.kv_offload,
        tensor_split: resolve_tensor_split(&model.tensor_split),
        split_mode: model.split_mode.clone(),
        vram_budget_mb: model.vram_budget_mb.clone(),
    }
}
