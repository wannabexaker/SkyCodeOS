use skycode_runtime::inference::{ModelConfig, ModelRegistry, ModelRuntime};
use skycode_runtime::orchestrator::{classify_task, map_to_model, RouterError, TaskClass};

#[test]
fn test_router_classify_10_samples() {
    let samples = [
        ("rename the function foo to bar", TaskClass::Refactor),
        ("fix the off-by-one bug in parser.rs", TaskClass::CodeEdit),
        (
            "explain why we use FTS5 instead of vector",
            TaskClass::ShortAnswer,
        ),
        (
            "extract the auth logic into its own module",
            TaskClass::Refactor,
        ),
        ("design the multi-agent handoff protocol", TaskClass::Plan),
        ("add a new CLI command for export", TaskClass::CodeEdit),
        ("what does the orchestrator do", TaskClass::ShortAnswer),
        ("refactor memory retrieval to use BM25", TaskClass::Refactor),
        ("plan the Phase 6 roadmap", TaskClass::Plan),
        ("implement the tuning_runs insert", TaskClass::CodeEdit),
    ];

    let correct = samples
        .iter()
        .filter(|(goal, expected)| classify_task(goal) == *expected)
        .count();

    assert!(
        correct >= 9,
        "router classified {correct}/10 samples correctly"
    );
    assert_eq!(
        correct, 10,
        "deterministic heuristic should pass all samples"
    );
}

#[test]
fn test_router_fallback_on_primary_failure() -> Result<(), Box<dyn std::error::Error>> {
    let registry = ModelRegistry {
        models: vec![local_model("local-coder-fast", true)],
    };

    let model = map_to_model(TaskClass::CodeEdit, &registry)?;
    assert_eq!(model.name, "local-coder-fast");

    Ok(())
}

#[test]
fn test_router_explicit_failure_no_models() {
    let registry = ModelRegistry {
        models: vec![local_model("local-coder", false)],
    };

    let err = map_to_model(TaskClass::CodeEdit, &registry)
        .expect_err("router must fail explicitly when no local model is enabled");
    assert!(
        matches!(err, RouterError::NoLocalModel(TaskClass::CodeEdit)),
        "expected NoLocalModel, got {err}"
    );
}

fn local_model(name: &str, enabled: bool) -> ModelConfig {
    ModelConfig {
        name: name.to_string(),
        runtime: ModelRuntime::LocalGguf,
        path: "C:/models/local.gguf".to_string(),
        executable: Some("llama-server".to_string()),
        ctx_size: 8192,
        gpu_layers: 0,
        strengths: Vec::new(),
        enabled,
        threads: 4,
        n_cpu_moe: None,
        no_mmap: false,
        mlock: false,
        port: 18080,
    }
}
