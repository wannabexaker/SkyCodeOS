use std::path::PathBuf;
use std::time::{Instant, SystemTime, UNIX_EPOCH};

use clap::{Args, Subcommand};
use ring::digest;
use rusqlite::{params, Connection};

use skycode_orchestrator::db::migrations::run_migrations;
use skycode_orchestrator::inference::{
    launch_model, resolve_gpu_layers, resolve_tensor_split, ModelLaunchOptions,
    ModelRegistryWatcher, ModelRuntime,
};

#[derive(Debug, Subcommand)]
pub enum ModelCommands {
    /// Load model from registry and launch local subprocess.
    Load(ModelNameArgs),
    /// Verify model file exists and subprocess starts successfully.
    Verify(ModelNameArgs),
    /// Run 3 benchmark prompts and record latency in tuning_runs.
    Bench(ModelNameArgs),
}

#[derive(Debug, Args)]
pub struct ModelNameArgs {
    pub name: String,
}

pub fn run_model_command(command: &ModelCommands) -> Result<(), Box<dyn std::error::Error>> {
    match command {
        ModelCommands::Load(args) => run_model_load(args),
        ModelCommands::Verify(args) => run_model_verify(args),
        ModelCommands::Bench(args) => run_model_bench(args),
    }
}

fn run_model_load(args: &ModelNameArgs) -> Result<(), Box<dyn std::error::Error>> {
    let mut watcher = load_registry()?;
    let _ = watcher.reload_if_changed()?;
    let config = watcher.model(&args.name)?;

    ensure_local_runtime(&config.name, &config.runtime)?;
    let launch = to_launch_options(&config, None);

    let mut handle = launch_model(&launch)?;
    let pid = handle.process.id();
    let mlock_verified = handle.mlock_verified;

    println!("Model loaded: {}", config.name);
    println!("  pid: {}", pid);
    println!("  mlock_verified: {}", mlock_verified);
    if let Some(w) = &handle.mlock_warning {
        println!("  mlock_warning: {}", w);
    }

    handle.stop()?;
    Ok(())
}

fn run_model_verify(args: &ModelNameArgs) -> Result<(), Box<dyn std::error::Error>> {
    let mut watcher = load_registry()?;
    let _ = watcher.reload_if_changed()?;
    let config = watcher.model(&args.name)?;

    ensure_local_runtime(&config.name, &config.runtime)?;

    let launch = to_launch_options(&config, Some("hello".to_string()));
    let mut handle = launch_model(&launch)?;
    std::thread::sleep(std::time::Duration::from_millis(250));

    let exited = handle.process.try_wait()?.is_some();
    if exited {
        return Err("model process exited too early during verify".into());
    }

    println!("Model verify OK:");
    println!("  name: {}", config.name);
    println!("  model_path: {}", launch.model_path.display());
    println!("  mlock_verified: {}", handle.mlock_verified);
    if let Some(w) = &handle.mlock_warning {
        println!("  mlock_warning: {}", w);
    }

    handle.stop()?;
    Ok(())
}

fn run_model_bench(args: &ModelNameArgs) -> Result<(), Box<dyn std::error::Error>> {
    let mut watcher = load_registry()?;
    let _ = watcher.reload_if_changed()?;
    let config = watcher.model(&args.name)?;

    ensure_local_runtime(&config.name, &config.runtime)?;

    let db_path = std::env::current_dir()?.join("skycode.db");
    let conn = Connection::open(db_path)?;

    let migrations_dir = std::env::current_dir()?.join("memory").join("migrations");
    if migrations_dir.exists() {
        let _ = run_migrations(&conn, &migrations_dir)?;
    }

    let prompts = [
        "Summarize this codebase in one line.",
        "List potential refactor opportunities.",
        "Generate a tiny patch plan for a safe rename.",
    ];

    for prompt in prompts {
        let launch = to_launch_options(&config, Some(prompt.to_string()));
        let start = Instant::now();
        let mut handle = launch_model(&launch)?;
        std::thread::sleep(std::time::Duration::from_millis(300));

        let elapsed_ms = i64::try_from(start.elapsed().as_millis()).unwrap_or(i64::MAX);
        let prompt_hash = sha256_hex(prompt.as_bytes());
        let now = unix_now()?;

        let run_id = format!("bench-{}-{}", config.name, now_nanos()?);
        let settings_json = format!(
            "{{\"ctx_size\":{},\"gpu_layers\":{},\"threads\":{}}}",
            config.ctx_size, launch.n_gpu_layers, config.threads
        );

        let mut stmt = conn.prepare(
            "INSERT INTO tuning_runs (
                id, project_id, model_name, profile_name, task_class, prompt_hash,
                settings_json, result_summary, first_token_ms, decode_tok_s,
                prompt_eval_tok_s, peak_vram_mb, peak_ram_mb, ctx_requested,
                ctx_achieved, output_tokens, error_code, created_at
             ) VALUES (
                ?1, ?2, ?3, ?4, ?5, ?6,
                ?7, ?8, ?9, ?10,
                ?11, ?12, ?13, ?14,
                ?15, ?16, ?17, ?18
             )",
        )?;

        stmt.execute(params![
            run_id,
            Option::<String>::None,
            config.name,
            "precise",
            Some("bench".to_string()),
            prompt_hash,
            settings_json,
            Some("phase3 bench".to_string()),
            Some(elapsed_ms),
            Option::<f64>::None,
            Option::<f64>::None,
            Option::<i64>::None,
            Option::<i64>::None,
            Some(i64::try_from(config.ctx_size).unwrap_or(i64::MAX)),
            Some(i64::try_from(config.ctx_size).unwrap_or(i64::MAX)),
            Option::<i64>::None,
            Option::<String>::None,
            now,
        ])?;

        handle.stop()?;

        println!("Bench run recorded:");
        println!("  model: {}", config.name);
        println!("  prompt_hash: {}", prompt_hash);
        println!("  latency_ms: {}", elapsed_ms);
    }

    Ok(())
}

fn ensure_local_runtime(
    name: &str,
    runtime: &ModelRuntime,
) -> Result<(), Box<dyn std::error::Error>> {
    if *runtime != ModelRuntime::LocalGguf {
        return Err(format!("model '{}' is not local_gguf", name).into());
    }
    Ok(())
}

fn to_launch_options(
    config: &skycode_orchestrator::inference::ModelConfig,
    prompt: Option<String>,
) -> ModelLaunchOptions {
    ModelLaunchOptions {
        executable: config.executable.clone(),
        model_path: PathBuf::from(&config.path),
        ctx_size: config.ctx_size,
        threads: config.threads,
        n_gpu_layers: resolve_gpu_layers(
            &config.gpu_layers,
            &PathBuf::from(&config.path),
            &config.vram_budget_mb,
        ),
        n_cpu_moe: config.n_cpu_moe,
        prompt,
        temp: 0.1,
        repeat_penalty: 1.1,
        max_tokens: 1024,
        no_mmap: config.no_mmap,
        mlock: config.mlock,
        port: config.port,
        kv_offload: config.kv_offload,
        tensor_split: resolve_tensor_split(&config.tensor_split),
        split_mode: config.split_mode.clone(),
        vram_budget_mb: config.vram_budget_mb.clone(),
    }
}

fn load_registry() -> Result<ModelRegistryWatcher, Box<dyn std::error::Error>> {
    let path = std::env::current_dir()?.join("agents").join("models.yaml");
    Ok(ModelRegistryWatcher::load(path)?)
}

fn sha256_hex(data: &[u8]) -> String {
    let hash = digest::digest(&digest::SHA256, data);
    let mut out = String::with_capacity(hash.as_ref().len() * 2);
    for b in hash.as_ref() {
        out.push(hex_char((b >> 4) & 0x0f));
        out.push(hex_char(b & 0x0f));
    }
    out
}

fn hex_char(value: u8) -> char {
    match value {
        0..=9 => (b'0' + value) as char,
        _ => (b'a' + (value - 10)) as char,
    }
}

fn unix_now() -> Result<i64, Box<dyn std::error::Error>> {
    let secs = SystemTime::now().duration_since(UNIX_EPOCH)?.as_secs();
    Ok(i64::try_from(secs)?)
}

fn now_nanos() -> Result<u128, Box<dyn std::error::Error>> {
    Ok(SystemTime::now().duration_since(UNIX_EPOCH)?.as_nanos())
}
