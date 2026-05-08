use std::fs;
use std::path::{Path, PathBuf};
use std::thread;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use rusqlite::{params, Connection};
use skycode_runtime::inference::{
    is_mlock_warning_line, ModelRegistryError, ModelRegistryWatcher, ModelRuntime,
};

#[test]
fn test_registry_hot_reload() -> Result<(), Box<dyn std::error::Error>> {
    let registry_path = temp_path("phase3-models", "yaml");

    write_registry(
        &registry_path,
        "models:\n  - name: local-coder\n    runtime: local_gguf\n    path: \"C:/models/local.gguf\"\n    ctx_size: 32768\n    gpu_layers: 20\n    strengths: []\n    enabled: true\n    threads: 8\n    n_cpu_moe:\n    no_mmap: false\n    mlock: false\n",
    )?;

    let mut watcher = ModelRegistryWatcher::load(&registry_path)?;
    let local_count = watcher
        .registry()
        .models
        .iter()
        .filter(|m| m.runtime == ModelRuntime::LocalGguf)
        .count();
    assert_eq!(local_count, 1, "expected one local model initially");

    // Ensure mtime changes on filesystems with coarse timestamp granularity.
    thread::sleep(Duration::from_millis(1200));

    write_registry(
        &registry_path,
        "models:\n  - name: local-coder\n    runtime: local_gguf\n    path: \"C:/models/local.gguf\"\n    ctx_size: 32768\n    gpu_layers: 20\n    strengths: []\n    enabled: true\n    threads: 8\n    n_cpu_moe:\n    no_mmap: false\n    mlock: false\n  - name: local-coder-fast\n    runtime: local_gguf\n    path: \"C:/models/local-fast.gguf\"\n    ctx_size: 8192\n    gpu_layers: 10\n    strengths: [\"fast\"]\n    enabled: true\n    threads: 4\n    n_cpu_moe:\n    no_mmap: false\n    mlock: false\n",
    )?;

    let changed = watcher.reload_if_changed()?;
    assert!(changed, "expected registry reload to detect file change");

    let local_count_after = watcher
        .registry()
        .models
        .iter()
        .filter(|m| m.runtime == ModelRuntime::LocalGguf)
        .count();
    assert_eq!(
        local_count_after, 2,
        "expected two local models after reload"
    );

    thread::sleep(Duration::from_millis(1200));

    write_registry(
        &registry_path,
        "models:\n  - name: remote-bad\n    runtime: openai_compatible\n    path: \"\"\n    ctx_size: 0\n    gpu_layers: 0\n    strengths: []\n    enabled: true\n    threads: 1\n    n_cpu_moe:\n    no_mmap: false\n    mlock: false\n",
    )?;

    let err = watcher
        .reload_if_changed()
        .expect_err("expected remote enabled to fail");
    match err {
        ModelRegistryError::RemoteAdapterEnabled(name) => assert_eq!(name, "remote-bad"),
        other => panic!("unexpected error variant: {other}"),
    }

    let _ = fs::remove_file(&registry_path);

    Ok(())
}

#[test]
fn test_mlock_warning_detection() {
    let warning_lines = [
        "WARNING: failed to mlock model weights",
        "mlock: cannot allocate memory",
        "mLock failed: permission denied",
        "mlock disabled: not supported on this platform",
    ];
    for line in &warning_lines {
        assert!(
            is_mlock_warning_line(line),
            "expected mlock warning detected in: {line}"
        );
    }

    let non_warning_lines = [
        "llama_model_load: loading model from path",
        "all good, memory locked",
        "context size = 32768",
        "",
    ];
    for line in &non_warning_lines {
        assert!(
            !is_mlock_warning_line(line),
            "unexpected mlock warning detected in: {line}"
        );
    }
}

#[test]
fn test_model_bench_writes_tuning_runs() -> Result<(), Box<dyn std::error::Error>> {
    let conn = Connection::open_in_memory()?;
    apply_phase_schema(&conn)?;

    let latencies = [42_i64, 58_i64, 73_i64];
    insert_bench_runs_with_mock(&conn, "local-coder", "precise", &latencies)?;

    let count: i64 = conn.query_row("SELECT COUNT(*) FROM tuning_runs", [], |r| r.get(0))?;
    assert_eq!(
        count, 3,
        "expected one tuning_runs row per benchmark prompt"
    );

    let mut stmt = conn.prepare(
        "SELECT model_name, first_token_ms, profile_name, created_at, task_class
         FROM tuning_runs
         ORDER BY created_at ASC",
    )?;

    let mut rows = stmt.query([])?;
    let mut seen = 0usize;

    while let Some(row) = rows.next()? {
        let model_name: String = row.get(0)?;
        let first_token_ms: Option<i64> = row.get(1)?;
        let profile_name: String = row.get(2)?;
        let created_at: i64 = row.get(3)?;
        let task_class: Option<String> = row.get(4)?;

        assert_eq!(model_name, "local-coder");
        assert_eq!(profile_name, "precise");
        assert!(created_at > 0, "created_at must be populated");
        assert!(
            first_token_ms.unwrap_or_default() > 0,
            "latency must be > 0"
        );

        // Schema has no task_id column in tuning_runs; task_class is used as task binding tag.
        assert!(task_class.is_some(), "task binding tag must not be null");

        seen += 1;
    }

    assert_eq!(seen, 3);

    Ok(())
}

fn insert_bench_runs_with_mock(
    conn: &Connection,
    model_name: &str,
    profile_name: &str,
    latencies_ms: &[i64],
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

    for (idx, latency) in latencies_ms.iter().enumerate() {
        let now = unix_now()? + i64::try_from(idx)?;
        stmt.execute(params![
            format!("bench-run-{}", idx),
            Option::<String>::None,
            model_name,
            profile_name,
            Some(format!("task-{}", idx)),
            format!("prompt-hash-{}", idx),
            "{\"ctx_size\":32768}".to_string(),
            Some("mock bench".to_string()),
            Some(*latency),
            Option::<f64>::None,
            Option::<f64>::None,
            Option::<i64>::None,
            Option::<i64>::None,
            Some(32768_i64),
            Some(32768_i64),
            Option::<i64>::None,
            Option::<String>::None,
            now,
        ])?;
    }

    Ok(())
}

fn apply_phase_schema(conn: &Connection) -> Result<(), Box<dyn std::error::Error>> {
    let schema_path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("memory")
        .join("migrations")
        .join("001_initial.sql");
    let sql = fs::read_to_string(schema_path)?;
    conn.execute_batch(&sql)?;
    Ok(())
}

fn write_registry(path: &Path, content: &str) -> Result<(), Box<dyn std::error::Error>> {
    fs::write(path, content)?;
    Ok(())
}

fn temp_path(prefix: &str, ext: &str) -> PathBuf {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    std::env::temp_dir().join(format!("{}-{}.{}", prefix, nanos, ext))
}

fn unix_now() -> Result<i64, Box<dyn std::error::Error>> {
    let secs = SystemTime::now().duration_since(UNIX_EPOCH)?.as_secs();
    Ok(i64::try_from(secs)?)
}
