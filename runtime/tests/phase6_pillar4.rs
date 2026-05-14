use std::path::PathBuf;

use skycode_runtime::inference::{
    build_llama_server_argv, ModelLaunchOptions, ModelRegistry, ModelRegistryError, ModelRuntime,
    SplitMode,
};

#[test]
fn phase6_tensor_split_valid() -> Result<(), Box<dyn std::error::Error>> {
    let registry = ModelRegistry::from_yaml(
        "models:
  - name: local-coder
    runtime: local_gguf
    path: C:/models/local.gguf
    ctx_size: 8192
    gpu_layers: 20
    strengths: []
    enabled: true
    threads: 8
    n_cpu_moe:
    no_mmap: false
    mlock: false
    tensor_split: [0.43, 0.57]
",
    )?;

    assert!(registry.model("local-coder").is_some());

    Ok(())
}

#[test]
fn phase6_tensor_split_invalid() {
    let err = ModelRegistry::from_yaml(
        "models:
  - name: local-coder
    runtime: local_gguf
    path: C:/models/local.gguf
    ctx_size: 8192
    gpu_layers: 20
    strengths: []
    enabled: true
    threads: 8
    n_cpu_moe:
    no_mmap: false
    mlock: false
    tensor_split: [0.5, 0.6]
",
    )
    .expect_err("tensor_split summing to 1.1 must be rejected");

    assert!(matches!(err, ModelRegistryError::InvalidTensorSplit(_)));
}

#[test]
fn phase6_existing_fields_preserved() -> Result<(), Box<dyn std::error::Error>> {
    let registry = ModelRegistry::from_yaml(
        "models:
  - name: local-coder
    runtime: local_gguf
    path: C:/models/local.gguf
    ctx_size: 8192
    gpu_layers: 20
    strengths: [code_edit]
    enabled: true
    threads: 8
    n_cpu_moe: 4
    no_mmap: true
    mlock: false
",
    )?;

    let model = registry.model("local-coder").ok_or("missing local-coder")?;

    assert_eq!(model.runtime, ModelRuntime::LocalGguf);
    assert_eq!(model.n_cpu_moe, Some(4));
    assert!(model.no_mmap);
    assert!(!model.mlock);
    assert_eq!(model.threads, 8);

    Ok(())
}

#[test]
fn phase6_llama_server_flag_compat() -> Result<(), Box<dyn std::error::Error>> {
    let options = ModelLaunchOptions {
        executable: Some("llama-server".to_string()),
        model_path: PathBuf::from("C:/models/local.gguf"),
        ctx_size: 8192,
        threads: 8,
        n_gpu_layers: 20,
        n_cpu_moe: None,
        prompt: None,
        temp: 0.1,
        repeat_penalty: 1.1,
        max_tokens: 1024,
        no_mmap: false,
        mlock: false,
        kv_offload: false,
        tensor_split: vec![0.4, 0.6],
        split_mode: SplitMode::Row,
        vram_budget_mb: None,
        port: 18080,
    };

    let argv = build_llama_server_argv(&options);

    assert!(argv.iter().any(|arg| arg == "--no-kv-offload"));
    assert_eq!(arg_value(&argv, "--tensor-split")?, "0.4,0.6");
    assert_eq!(arg_value(&argv, "--split-mode")?, "row");
    assert_eq!(arg_value(&argv, "--n-gpu-layers")?, "20");

    Ok(())
}

fn arg_value<'a>(argv: &'a [String], flag: &str) -> Result<&'a str, Box<dyn std::error::Error>> {
    argv.windows(2)
        .find(|window| window[0] == flag)
        .map(|window| window[1].as_str())
        .ok_or_else(|| format!("missing argv flag {flag}").into())
}

/// Auto GPU layer split: pure computation functions verified with synthetic inputs.
/// No GPU hardware required because these functions are pure.
#[test]
fn phase6_auto_layer_split() {
    use skycode_runtime::inference::{auto_tensor_split_from_gpus, compute_auto_gpu_layers};
    use skycode_runtime::tools::hardware::GpuInfo;

    assert_eq!(
        compute_auto_gpu_layers(4000, 6000, 32),
        32,
        "all layers of a 7B model should fit in 6 GB"
    );

    let layers = compute_auto_gpu_layers(40000, 6000, 80);
    assert!(
        layers >= 8 && layers <= 13,
        "expected about 10 layers of a 70B model to fit in 6 GB, got {layers}"
    );

    assert_eq!(
        compute_auto_gpu_layers(4000, 0, 32),
        0,
        "zero VRAM must result in zero GPU layers"
    );

    let gpus = vec![
        GpuInfo {
            index: 0,
            name: "GPU-A".into(),
            vram_total_mb: 6144,
            vram_free_mb: 5500,
        },
        GpuInfo {
            index: 1,
            name: "GPU-B".into(),
            vram_total_mb: 8192,
            vram_free_mb: 7500,
        },
    ];
    let split = auto_tensor_split_from_gpus(&gpus);
    assert_eq!(split.len(), 2, "expected 2 split ratios for 2 GPUs");
    let sum: f64 = split.iter().sum();
    assert!(
        (sum - 1.0).abs() < 0.001,
        "tensor_split must sum to 1.0, got {sum:.6}"
    );
    assert!(split[0] < split[1], "smaller GPU must get smaller ratio");
    assert!(
        split[0] > 0.3 && split[0] < 0.5,
        "6 GB ratio is about 0.43, got {:.3}",
        split[0]
    );

    let single = vec![GpuInfo {
        index: 0,
        name: "GPU".into(),
        vram_total_mb: 8192,
        vram_free_mb: 7500,
    }];
    assert!(
        auto_tensor_split_from_gpus(&single).is_empty(),
        "single GPU must return empty tensor_split"
    );

    assert!(
        auto_tensor_split_from_gpus(&[]).is_empty(),
        "no GPUs must return empty tensor_split"
    );
}

/// detect_gpus() must never panic and must return structurally valid results.
/// On CPU-only CI machines it returns an empty Vec — that is expected and correct.
#[test]
fn phase6_hardware_detect_no_panic() {
    let gpus = skycode_runtime::tools::hardware::detect_gpus();
    for gpu in &gpus {
        assert!(
            gpu.vram_total_mb > 0,
            "GPU must report non-zero total VRAM: {gpu:?}"
        );
        assert!(
            !gpu.name.is_empty(),
            "GPU must have a non-empty name: {gpu:?}"
        );
        assert!(
            gpu.vram_free_mb <= gpu.vram_total_mb,
            "free VRAM cannot exceed total: {gpu:?}"
        );
    }
    // Always passes — the critical invariant is that detect_gpus() does not panic.
}
