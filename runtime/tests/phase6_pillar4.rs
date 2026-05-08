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
