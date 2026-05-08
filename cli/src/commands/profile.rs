use clap::{Args, Subcommand};
use ring::digest;
use rusqlite::{params, Connection, OptionalExtension};
use serde_json::{json, Value};
use std::time::{Instant, SystemTime, UNIX_EPOCH};
use uuid::Uuid;

use skycode_orchestrator::db::migrations::run_migrations;
use skycode_orchestrator::inference::{ModelConfig, ModelRegistryWatcher, ModelRuntime};
use skycode_orchestrator::orchestrator::{
    classify_task, map_to_model, run_task_loop, TaskClass, TaskLoopInput,
};

#[derive(Debug, Subcommand)]
pub enum ProfileCommands {
    /// Set the active tuning profile for this project.
    Use(ProfileUseArgs),
    /// Show the current active profile.
    Show,
    /// Run one task and record timing to tuning_runs.
    Bench(ProfileBenchArgs),
    /// Compare two tuning_run rows.
    Compare(ProfileCompareArgs),
    /// Sweep a standard task suite.
    Tune(ProfileTuneArgs),
    /// Dump tuning_runs to stdout.
    ExportResults(ProfileExportArgs),
}

#[derive(Debug, Args)]
pub struct ProfileUseArgs {
    /// Profile name: precise | fast | creative | deep
    pub profile: String,

    /// Set the test command run after apply (e.g. "cargo test").
    #[arg(long)]
    pub test_command: Option<String>,

    /// Set the verify timeout in seconds (1–300, default 60).
    #[arg(long)]
    pub verify_timeout: Option<u64>,
}

#[derive(Debug, Args)]
pub struct ProfileBenchArgs {
    /// Task to run through the profile benchmark path.
    pub task: String,
    /// Tuning profile override.
    #[arg(long)]
    pub profile: Option<String>,
    /// Model override.
    #[arg(long)]
    pub model: Option<String>,
}

#[derive(Debug, Args)]
pub struct ProfileCompareArgs {
    pub run_id1: String,
    pub run_id2: String,
}

#[derive(Debug, Args)]
pub struct ProfileTuneArgs {
    /// Model override.
    #[arg(long)]
    pub model: Option<String>,
    /// Tuning profile override.
    #[arg(long)]
    pub profile: Option<String>,
}

#[derive(Debug, Args)]
pub struct ProfileExportArgs {
    /// Output format: csv | json.
    #[arg(long, default_value = "json")]
    pub format: String,
    /// Maximum rows to export.
    #[arg(long, default_value_t = 50)]
    pub limit: usize,
}

struct TuningRunRecord {
    id: String,
    project_id: Option<String>,
    model_name: String,
    profile_name: String,
    task_class: String,
    prompt_hash: String,
    settings_json: String,
    result_summary: Option<String>,
    first_token_ms: Option<i64>,
    output_tokens: Option<i64>,
    error_code: Option<String>,
    created_at: i64,
}

pub fn run_profile_command(cmd: &ProfileCommands) -> Result<(), Box<dyn std::error::Error>> {
    match cmd {
        ProfileCommands::Use(args) => run_profile_use(args),
        ProfileCommands::Show => run_profile_show(),
        ProfileCommands::Bench(args) => run_profile_bench(args),
        ProfileCommands::Compare(args) => run_profile_compare(args),
        ProfileCommands::Tune(args) => run_profile_tune(args),
        ProfileCommands::ExportResults(args) => run_profile_export(args),
    }
}

fn run_profile_use(args: &ProfileUseArgs) -> Result<(), Box<dyn std::error::Error>> {
    if !is_valid_profile(&args.profile) {
        return Err(format!(
            "invalid profile '{}'; expected precise, fast, creative, or deep",
            args.profile
        )
        .into());
    }

    let conn = open_db_with_migrations()?;
    let mut stmt = conn.prepare(
        "INSERT INTO agent_state (agent_id, project_id, state_json, session_id, updated_at)
         VALUES ('coder-primary', 'default', json_object('profile', ?1), 'cli', unixepoch())
         ON CONFLICT(agent_id, project_id) DO UPDATE SET
           state_json = json_set(
             COALESCE(state_json, '{}'),
             '$.profile',
             excluded.state_json->>'$.profile'
           ),
           updated_at = excluded.updated_at",
    )?;
    stmt.execute(params![args.profile])?;

    println!("Profile set to '{}'.", args.profile);

    if let Some(cmd) = &args.test_command {
        let changed = conn.execute(
            "UPDATE agent_state
             SET test_command = ?1, updated_at = unixepoch()
             WHERE agent_id = 'coder-primary' AND project_id = 'default'",
            params![cmd],
        )?;

        if changed == 0 {
            conn.execute(
                "INSERT INTO agent_state (
                    agent_id, project_id, state_json, session_id, updated_at, test_command, verify_timeout_secs
                 ) VALUES (
                    'coder-primary', 'default', '{}', 'cli', unixepoch(), ?1, 60
                 )",
                params![cmd],
            )?;
        }

        println!("test_command set to: {}", cmd);
    }

    if let Some(timeout) = args.verify_timeout {
        let clamped = timeout.clamp(1, 300);
        let changed = conn.execute(
            "UPDATE agent_state
             SET verify_timeout_secs = ?1, updated_at = unixepoch()
             WHERE agent_id = 'coder-primary' AND project_id = 'default'",
            params![i64::try_from(clamped)?],
        )?;

        if changed == 0 {
            conn.execute(
                "INSERT INTO agent_state (
                    agent_id, project_id, state_json, session_id, updated_at, test_command, verify_timeout_secs
                 ) VALUES (
                    'coder-primary', 'default', '{}', 'cli', unixepoch(), NULL, ?1
                 )",
                params![i64::try_from(clamped)?],
            )?;
        }

        println!("verify_timeout set to: {}s", clamped);
    }

    Ok(())
}

fn run_profile_show() -> Result<(), Box<dyn std::error::Error>> {
    let conn = open_db_with_migrations()?;
    let state_json: Option<String> = conn
        .query_row(
            "SELECT state_json FROM agent_state
             WHERE agent_id = 'coder-primary' AND project_id = 'default'",
            [],
            |r| r.get(0),
        )
        .optional()?;

    let profile = state_json
        .as_deref()
        .and_then(|s| serde_json::from_str::<Value>(s).ok())
        .and_then(|v| v.get("profile").and_then(Value::as_str).map(str::to_string));

    match profile {
        Some(profile) => println!("{profile}"),
        None => println!("precise (default)"),
    }

    Ok(())
}

fn run_profile_bench(args: &ProfileBenchArgs) -> Result<(), Box<dyn std::error::Error>> {
    let conn = open_db_with_migrations()?;
    let record = run_tuning_task(
        &conn,
        &args.task,
        args.profile.as_deref(),
        args.model.as_deref(),
    )?;
    best_effort_insert_tuning_run(&conn, &record);

    println!("run_id: {}", record.id);
    println!("model: {}", record.model_name);
    println!("profile: {}", record.profile_name);
    println!(
        "first_token_ms: {}",
        record
            .first_token_ms
            .map(|v| v.to_string())
            .unwrap_or_else(|| "null".to_string())
    );
    println!(
        "output_tokens: {}",
        record
            .output_tokens
            .map(|v| v.to_string())
            .unwrap_or_else(|| "null".to_string())
    );

    if let Some(error_code) = record.error_code {
        return Err(error_code.into());
    }

    Ok(())
}

fn run_profile_compare(args: &ProfileCompareArgs) -> Result<(), Box<dyn std::error::Error>> {
    let conn = open_db_with_migrations()?;
    let a = load_tuning_run(&conn, &args.run_id1)?;
    let b = load_tuning_run(&conn, &args.run_id2)?;

    println!("{:<14} {:<18} {:<18}", "", "run-a", "run-b");
    println!("{:<14} {:<18} {:<18}", "model:", a.model_name, b.model_name);
    println!(
        "{:<14} {:<18} {:<18}",
        "profile:", a.profile_name, b.profile_name
    );
    println!(
        "{:<14} {:<18} {:<18}",
        "first_tok_ms:",
        a.first_token_ms
            .map(|v| v.to_string())
            .unwrap_or_else(|| "null".to_string()),
        b.first_token_ms
            .map(|v| v.to_string())
            .unwrap_or_else(|| "null".to_string())
    );
    println!(
        "{:<14} {:<18} {:<18}",
        "output_tok:",
        a.output_tokens
            .map(|v| v.to_string())
            .unwrap_or_else(|| "null".to_string()),
        b.output_tokens
            .map(|v| v.to_string())
            .unwrap_or_else(|| "null".to_string())
    );
    println!(
        "{:<14} {:<18} {:<18}",
        "task_class:", a.task_class, b.task_class
    );

    Ok(())
}

fn run_profile_tune(args: &ProfileTuneArgs) -> Result<(), Box<dyn std::error::Error>> {
    let conn = open_db_with_migrations()?;
    let tasks = [
        "add a hello world function to src/lib.rs",
        "fix the off-by-one bug in the loop",
        "explain what the orchestrator does",
        "refactor the scan function into smaller helpers",
        "plan the error handling strategy",
    ];

    println!(
        "{:<54} {:<14} {:<14} {:<12}",
        "task", "class", "first_tok_ms", "output_tok"
    );
    for task in tasks {
        let record = run_tuning_task(&conn, task, args.profile.as_deref(), args.model.as_deref())?;
        best_effort_insert_tuning_run(&conn, &record);
        println!(
            "{:<54} {:<14} {:<14} {:<12}",
            truncate(task, 54),
            record.task_class,
            record
                .first_token_ms
                .map(|v| v.to_string())
                .unwrap_or_else(|| "null".to_string()),
            record
                .output_tokens
                .map(|v| v.to_string())
                .unwrap_or_else(|| "null".to_string())
        );
        if let Some(summary) = &record.result_summary {
            println!("diff proposal: {summary}");
        }
    }

    Ok(())
}

fn run_profile_export(args: &ProfileExportArgs) -> Result<(), Box<dyn std::error::Error>> {
    let conn = open_db_with_migrations()?;
    let rows = load_recent_tuning_runs(&conn, args.limit)?;

    match args.format.as_str() {
        "json" => {
            let values: Vec<Value> = rows.iter().map(tuning_run_to_json).collect();
            println!("{}", serde_json::to_string_pretty(&values)?);
        }
        "csv" => {
            println!(
                "id,project_id,model_name,profile_name,task_class,prompt_hash,settings_json,result_summary,first_token_ms,output_tokens,error_code,created_at"
            );
            for row in rows {
                println!(
                    "{},{},{},{},{},{},{},{},{},{},{},{}",
                    csv_cell(&row.id),
                    csv_cell(row.project_id.as_deref().unwrap_or("")),
                    csv_cell(&row.model_name),
                    csv_cell(&row.profile_name),
                    csv_cell(&row.task_class),
                    csv_cell(&row.prompt_hash),
                    csv_cell(&row.settings_json),
                    csv_cell(row.result_summary.as_deref().unwrap_or("")),
                    row.first_token_ms
                        .map(|v| v.to_string())
                        .unwrap_or_default(),
                    row.output_tokens.map(|v| v.to_string()).unwrap_or_default(),
                    csv_cell(row.error_code.as_deref().unwrap_or("")),
                    row.created_at
                );
            }
        }
        other => {
            return Err(format!("invalid export format '{other}'; expected csv or json").into())
        }
    }

    Ok(())
}

fn run_tuning_task(
    conn: &Connection,
    task: &str,
    profile_override: Option<&str>,
    model_override: Option<&str>,
) -> Result<TuningRunRecord, Box<dyn std::error::Error>> {
    let profile = match profile_override {
        Some(profile) => {
            if !is_valid_profile(profile) {
                return Err(format!(
                    "invalid profile '{profile}'; expected precise, fast, creative, or deep"
                )
                .into());
            }
            profile.to_string()
        }
        None => load_saved_profile(conn)?.unwrap_or_else(|| "precise".to_string()),
    };
    let task_class = classify_task(task);
    let model = resolve_model(task_class, model_override)?;
    let prompt_proxy = format!("task:{task}\nmodel:{}\nprofile:{profile}", model.name);
    let settings_json = serde_json::to_string(&json!({
        "preferred": model.name,
        "fallback": "local-coder-fast",
        "profile": profile,
    }))?;
    let task_id = format!("profile-bench-{}", Uuid::new_v4());
    let input = TaskLoopInput {
        task_id,
        project_id: "default".to_string(),
        goal: task.to_string(),
        repo_root: ".".to_string(),
        profile: profile.clone(),
    };

    let start = Instant::now();
    let outcome = run_task_loop(conn, &input);
    let elapsed = elapsed_ms(start)?;
    let (result_summary, output_tokens, error_code) = match outcome {
        Ok(output) => {
            println!("Proposed diff:");
            println!("  id: {}", output.diff.id);
            println!("  file: {}", output.diff.file_path);
            println!("---");
            println!("{}", output.diff.diff_text);
            let token_count = whitespace_count(&output.response_summary)
                + whitespace_count(&output.diff.diff_text);
            (Some(output.response_summary), Some(token_count), None)
        }
        Err(err) => {
            let message = err.to_string();
            (
                Some(message.clone()),
                Some(whitespace_count(&message)),
                Some(message),
            )
        }
    };

    Ok(TuningRunRecord {
        id: Uuid::new_v4().to_string(),
        project_id: Some("default".to_string()),
        model_name: model.name,
        profile_name: profile,
        task_class: task_class_name(task_class).to_string(),
        prompt_hash: sha256_hex(prompt_proxy.as_bytes()),
        settings_json,
        result_summary,
        first_token_ms: Some(elapsed),
        output_tokens,
        error_code,
        created_at: unix_now()?,
    })
}

fn resolve_model(
    task_class: TaskClass,
    model_override: Option<&str>,
) -> Result<ModelConfig, Box<dyn std::error::Error>> {
    let models_path = std::env::current_dir()?.join("agents").join("models.yaml");
    let mut watcher = ModelRegistryWatcher::load(models_path)?;
    let _ = watcher.reload_if_changed()?;

    if let Some(name) = model_override {
        let model = watcher.model(name)?;
        if model.runtime != ModelRuntime::LocalGguf || !model.enabled {
            return Err(format!("model '{name}' is not an enabled local_gguf model").into());
        }
        return Ok(model);
    }

    Ok(map_to_model(task_class, watcher.registry())?.clone())
}

fn best_effort_insert_tuning_run(conn: &Connection, record: &TuningRunRecord) {
    if let Err(err) = insert_tuning_run(conn, record) {
        eprintln!("warning: failed to insert tuning_run: {err}");
    }
}

fn insert_tuning_run(
    conn: &Connection,
    record: &TuningRunRecord,
) -> Result<(), Box<dyn std::error::Error>> {
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
        record.id,
        record.project_id,
        record.model_name,
        record.profile_name,
        record.task_class,
        record.prompt_hash,
        record.settings_json,
        record.result_summary,
        record.first_token_ms,
        Option::<f64>::None,
        Option::<f64>::None,
        Option::<i64>::None,
        Option::<i64>::None,
        Option::<i64>::None,
        Option::<i64>::None,
        record.output_tokens,
        record.error_code,
        record.created_at,
    ])?;
    Ok(())
}

fn load_tuning_run(
    conn: &Connection,
    id: &str,
) -> Result<TuningRunRecord, Box<dyn std::error::Error>> {
    let mut stmt = conn.prepare(
        "SELECT id, project_id, model_name, profile_name, task_class, prompt_hash,
                settings_json, result_summary, first_token_ms, output_tokens,
                error_code, created_at
           FROM tuning_runs
          WHERE id = ?1",
    )?;

    Ok(stmt.query_row(params![id], row_to_tuning_run)?)
}

fn load_recent_tuning_runs(
    conn: &Connection,
    limit: usize,
) -> Result<Vec<TuningRunRecord>, Box<dyn std::error::Error>> {
    let mut stmt = conn.prepare(
        "SELECT id, project_id, model_name, profile_name, task_class, prompt_hash,
                settings_json, result_summary, first_token_ms, output_tokens,
                error_code, created_at
           FROM tuning_runs
          ORDER BY created_at DESC
          LIMIT ?1",
    )?;
    let rows = stmt.query_map(params![limit as i64], row_to_tuning_run)?;
    let mut out = Vec::new();
    for row in rows {
        out.push(row?);
    }
    Ok(out)
}

fn row_to_tuning_run(row: &rusqlite::Row<'_>) -> rusqlite::Result<TuningRunRecord> {
    Ok(TuningRunRecord {
        id: row.get(0)?,
        project_id: row.get(1)?,
        model_name: row.get(2)?,
        profile_name: row.get(3)?,
        task_class: row.get::<_, Option<String>>(4)?.unwrap_or_default(),
        prompt_hash: row.get(5)?,
        settings_json: row.get(6)?,
        result_summary: row.get(7)?,
        first_token_ms: row.get(8)?,
        output_tokens: row.get(9)?,
        error_code: row.get(10)?,
        created_at: row.get(11)?,
    })
}

fn load_saved_profile(conn: &Connection) -> Result<Option<String>, Box<dyn std::error::Error>> {
    let state_json: Option<String> = conn
        .query_row(
            "SELECT state_json FROM agent_state
             WHERE agent_id = 'coder-primary' AND project_id = 'default'",
            [],
            |r| r.get(0),
        )
        .optional()?;

    Ok(state_json
        .as_deref()
        .and_then(|s| serde_json::from_str::<Value>(s).ok())
        .and_then(|v| v.get("profile").and_then(Value::as_str).map(str::to_string)))
}

fn open_db_with_migrations() -> Result<Connection, Box<dyn std::error::Error>> {
    let db_path = std::env::current_dir()?.join("skycode.db");
    let conn = Connection::open(db_path)?;

    let migrations_dir = std::env::current_dir()?.join("memory").join("migrations");
    if migrations_dir.exists() {
        let _ = run_migrations(&conn, &migrations_dir)?;
    }

    Ok(conn)
}

fn is_valid_profile(profile: &str) -> bool {
    matches!(profile, "precise" | "fast" | "creative" | "deep")
}

fn task_class_name(class: TaskClass) -> &'static str {
    match class {
        TaskClass::CodeEdit => "code_edit",
        TaskClass::ShortAnswer => "short_answer",
        TaskClass::Refactor => "refactor",
        TaskClass::Plan => "plan",
    }
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

fn elapsed_ms(start: Instant) -> Result<i64, Box<dyn std::error::Error>> {
    Ok(i64::try_from(start.elapsed().as_millis())?)
}

fn unix_now() -> Result<i64, Box<dyn std::error::Error>> {
    let secs = SystemTime::now().duration_since(UNIX_EPOCH)?.as_secs();
    Ok(i64::try_from(secs)?)
}

fn whitespace_count(value: &str) -> i64 {
    match i64::try_from(value.split_whitespace().count()) {
        Ok(count) => count,
        Err(_) => i64::MAX,
    }
}

fn truncate(value: &str, max_chars: usize) -> String {
    if value.chars().count() <= max_chars {
        return value.to_string();
    }

    value
        .chars()
        .take(max_chars.saturating_sub(3))
        .collect::<String>()
        + "..."
}

fn tuning_run_to_json(row: &TuningRunRecord) -> Value {
    json!({
        "id": row.id,
        "project_id": row.project_id,
        "model_name": row.model_name,
        "profile_name": row.profile_name,
        "task_class": row.task_class,
        "prompt_hash": row.prompt_hash,
        "settings_json": row.settings_json,
        "result_summary": row.result_summary,
        "first_token_ms": row.first_token_ms,
        "output_tokens": row.output_tokens,
        "error_code": row.error_code,
        "created_at": row.created_at,
    })
}

fn csv_cell(value: &str) -> String {
    let escaped = value.replace('"', "\"\"");
    format!("\"{escaped}\"")
}
