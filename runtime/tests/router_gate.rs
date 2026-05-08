//! Router gate tests — verifies classify_task and map_to_model behaviour.

use skycode_runtime::inference::registry::ModelRegistry;
use skycode_runtime::orchestrator::router::{classify_task, map_to_model, RouterError, TaskClass};

// ─── helpers ─────────────────────────────────────────────────────────────────

fn make_registry(yaml: &str) -> ModelRegistry {
    ModelRegistry::from_yaml(yaml).expect("valid registry YAML")
}

fn local_gguf_entry(name: &str, enabled: bool) -> String {
    format!(
        "  - name: {name}\n    \
             runtime: local_gguf\n    \
             path: \"/models/{name}.gguf\"\n    \
             ctx_size: 8192\n    \
             gpu_layers: 0\n    \
             strengths: []\n    \
             enabled: {enabled}\n    \
             threads: 4\n    \
             n_cpu_moe:\n    \
             no_mmap: false\n    \
             mlock: false\n",
        name = name,
        enabled = enabled
    )
}

fn registry_yaml(entries: &[(&str, bool)]) -> String {
    let mut yaml = "models:\n".to_string();
    for (name, enabled) in entries {
        yaml.push_str(&local_gguf_entry(name, *enabled));
    }
    yaml
}

// ─── tests ───────────────────────────────────────────────────────────────────

/// 10 hand-labelled goals → assert correct TaskClass for each.
#[test]
fn test_router_classifies_correctly() {
    let cases: &[(&str, fn(&TaskClass) -> bool)] = &[
        ("refactor the auth module", |c| {
            matches!(c, TaskClass::Refactor)
        }),
        ("rename Foo to Bar", |c| matches!(c, TaskClass::Refactor)),
        ("move the handler to handlers/", |c| {
            matches!(c, TaskClass::Refactor)
        }),
        ("extract the helper function", |c| {
            matches!(c, TaskClass::Refactor)
        }),
        ("fix the login bug", |c| matches!(c, TaskClass::CodeEdit)),
        ("there is a crash in token parsing", |c| {
            matches!(c, TaskClass::CodeEdit)
        }),
        ("explain how the cache works", |c| {
            matches!(c, TaskClass::ShortAnswer)
        }),
        ("what does this function return", |c| {
            matches!(c, TaskClass::ShortAnswer)
        }),
        ("design the new database schema", |c| {
            matches!(c, TaskClass::Plan)
        }),
        ("architect the permission layer", |c| {
            matches!(c, TaskClass::Plan)
        }),
    ];

    for (goal, check) in cases {
        let class = classify_task(goal);
        assert!(
            check(&class),
            "classify_task({goal:?}) returned wrong class: {class:?}"
        );
    }
}

/// When local-coder is absent but local-coder-fast is present,
/// a non-ShortAnswer class must fall back to local-coder-fast.
#[test]
fn test_router_fallback_fires() -> Result<(), Box<dyn std::error::Error>> {
    // Only local-coder-fast is available
    let yaml = registry_yaml(&[("local-coder-fast", true)]);
    let registry = make_registry(&yaml);

    // CodeEdit prefers local-coder → not found → should fall back to local-coder-fast
    let model = map_to_model(TaskClass::CodeEdit, &registry)?;
    assert_eq!(
        model.name, "local-coder-fast",
        "expected fallback to local-coder-fast when local-coder is absent"
    );

    // Refactor likewise
    let model = map_to_model(TaskClass::Refactor, &registry)?;
    assert_eq!(model.name, "local-coder-fast");

    // ShortAnswer already prefers local-coder-fast → direct hit
    let model = map_to_model(TaskClass::ShortAnswer, &registry)?;
    assert_eq!(model.name, "local-coder-fast");

    Ok(())
}

/// When all local models are removed (or disabled), map_to_model returns Err
/// instead of silently routing to the remote adapter.
#[test]
fn test_router_no_silent_remote() {
    // Registry with no enabled local models
    let yaml = registry_yaml(&[("local-coder", false), ("local-coder-fast", false)]);
    let registry = make_registry(&yaml);

    for class in [
        TaskClass::CodeEdit,
        TaskClass::ShortAnswer,
        TaskClass::Refactor,
        TaskClass::Plan,
    ] {
        let err = map_to_model(class, &registry)
            .expect_err("expected Err when no local model is available");

        assert!(
            matches!(err, RouterError::NoLocalModel(_)),
            "expected NoLocalModel, got: {err}"
        );
    }
}
