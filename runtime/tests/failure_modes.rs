//! Failure-mode tests — verifies the system rejects invalid inputs safely.

use std::fs;
use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

use ring::rand::SystemRandom;
use ring::signature::Ed25519KeyPair;
use rusqlite::Connection;
use skycode_runtime::approval::token::{public_key_bytes, ApprovalToken};
use skycode_runtime::approval::validator::{register_signing_key, validate_token, ValidatorError};
use skycode_runtime::db::migrations::run_migrations;
use skycode_runtime::inference::registry::{ModelRegistry, ModelRegistryError};
use skycode_runtime::orchestrator::router::{map_to_model, RouterError, TaskClass};
use skycode_runtime::tools::diff::create_diff;

// ─── helpers ─────────────────────────────────────────────────────────────────

fn bootstrap_conn() -> Result<Connection, Box<dyn std::error::Error>> {
    let conn = Connection::open_in_memory()?;

    let migrations_dir = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("memory")
        .join("migrations");

    if migrations_dir.exists() {
        run_migrations(&conn, &migrations_dir)?;
    }

    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS approval_tokens_used (
            tid     TEXT PRIMARY KEY,
            diff_id TEXT NOT NULL,
            task_id TEXT NOT NULL,
            used_at INTEGER NOT NULL
        ) STRICT;

        CREATE TABLE IF NOT EXISTS signing_keys (
            agent_id       TEXT PRIMARY KEY,
            key_id         TEXT,
            public_key_hex TEXT NOT NULL,
            registered_at  INTEGER NOT NULL
        ) STRICT;",
    )?;

    Ok(conn)
}

fn make_keypair() -> Result<(Ed25519KeyPair, Vec<u8>), Box<dyn std::error::Error>> {
    let rng = SystemRandom::new();
    let pkcs8 = Ed25519KeyPair::generate_pkcs8(&rng).map_err(|_| "keygen failed")?;
    let kp = Ed25519KeyPair::from_pkcs8(pkcs8.as_ref()).map_err(|_| "key parse failed")?;
    let pub_key = public_key_bytes(&kp);
    Ok((kp, pub_key))
}

fn temp_yaml_path(label: &str) -> std::path::PathBuf {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    std::env::temp_dir().join(format!("{}-{}.yaml", label, nanos))
}

// ─── tests ───────────────────────────────────────────────────────────────────

/// A token with TTL=0 (already expired) must be rejected as Expired.
#[test]
fn test_expired_token_rejected() -> Result<(), Box<dyn std::error::Error>> {
    let conn = bootstrap_conn()?;
    let (kp, _pub_key) = make_keypair()?;
    let diff = create_diff("default", Path::new("test.rs"), "old", "new")?;

    // Create a normally-signed token, then force expires_at into the past.
    let mut token = ApprovalToken::create_signed(
        "default",
        diff.id.to_string(),
        "coder-primary",
        "coder-primary",
        "nonce-exp",
        &kp,
    )?;
    token.expires_at = 0; // epoch zero — definitely in the past

    let err = validate_token(
        &conn,
        &token,
        "default",
        &diff.id.to_string(),
        "coder-primary",
        "task-exp",
    )
    .expect_err("expected Expired error");

    assert!(
        matches!(err, ValidatorError::Expired),
        "expected Expired, got: {err}"
    );

    Ok(())
}

/// Using the same token twice must return ReplayDetected on the second use.
#[test]
fn test_replay_attack_blocked() -> Result<(), Box<dyn std::error::Error>> {
    let conn = bootstrap_conn()?;
    let (kp, pub_key) = make_keypair()?;
    let diff = create_diff("default", Path::new("replay.rs"), "before", "after")?;

    // Register the signing key so validate_token can look it up.
    let key_hex: String = pub_key.iter().map(|b| format!("{b:02x}")).collect();
    register_signing_key(&conn, "coder-primary", &key_hex, 0)?;

    let token = ApprovalToken::create_signed(
        "default",
        diff.id.to_string(),
        "coder-primary",
        "coder-primary",
        "nonce-replay",
        &kp,
    )?;

    // First use — must succeed
    validate_token(
        &conn,
        &token,
        "default",
        &diff.id.to_string(),
        "coder-primary",
        "task-r1",
    )?;

    // Second use — must be rejected
    let err = validate_token(
        &conn,
        &token,
        "default",
        &diff.id.to_string(),
        "coder-primary",
        "task-r2",
    )
    .expect_err("expected ReplayDetected on second use");

    assert!(
        matches!(err, ValidatorError::ReplayDetected),
        "expected ReplayDetected, got: {err}"
    );

    Ok(())
}

/// When no local model exists in the registry, map_to_model returns an explicit
/// Err — it does NOT silently fall back to the remote adapter.
#[test]
fn test_missing_model_explicit_err() -> Result<(), Box<dyn std::error::Error>> {
    // Registry with only a disabled local model
    let yaml = "models:\n  \
                - name: local-coder\n    \
                  runtime: local_gguf\n    \
                  path: \"nonexistent.gguf\"\n    \
                  ctx_size: 4096\n    \
                  gpu_layers: 0\n    \
                  strengths: []\n    \
                  enabled: false\n    \
                  threads: 4\n    \
                  n_cpu_moe:\n    \
                  no_mmap: false\n    \
                  mlock: false\n";

    let registry = ModelRegistry::from_yaml(yaml)?;

    // None of the local models are enabled, so every class should fail
    for class in [
        TaskClass::CodeEdit,
        TaskClass::ShortAnswer,
        TaskClass::Refactor,
        TaskClass::Plan,
    ] {
        let err = map_to_model(class, &registry).expect_err("expected NoLocalModel error");

        assert!(
            matches!(err, RouterError::NoLocalModel(_)),
            "expected NoLocalModel, got: {err}"
        );

        // Confirm no remote model was chosen (remote adapter must never be returned)
        // The above assertion already guarantees this — any return would have been Ok.
    }

    Ok(())
}

/// A registry YAML containing an *enabled* remote adapter must be rejected
/// at parse time with RemoteAdapterEnabled — it cannot be force-enabled at runtime.
#[test]
fn test_remote_adapter_disabled() -> Result<(), Box<dyn std::error::Error>> {
    let yaml_with_enabled_remote = "models:\n  \
                                    - name: bad-remote\n    \
                                      runtime: openai_compatible\n    \
                                      path: \"\"\n    \
                                      ctx_size: 0\n    \
                                      gpu_layers: 0\n    \
                                      strengths: []\n    \
                                      enabled: true\n    \
                                      threads: 1\n    \
                                      n_cpu_moe:\n    \
                                      no_mmap: false\n    \
                                      mlock: false\n";

    let err = ModelRegistry::from_yaml(yaml_with_enabled_remote)
        .expect_err("expected error for enabled remote adapter");

    assert!(
        matches!(err, ModelRegistryError::RemoteAdapterEnabled(_)),
        "expected RemoteAdapterEnabled, got: {err}"
    );

    Ok(())
}

/// Malformed models.yaml must produce a parse error, not a panic.
#[test]
fn test_invalid_yaml_registry() -> Result<(), Box<dyn std::error::Error>> {
    let path = temp_yaml_path("bad-registry");
    fs::write(&path, b"models: [\x00\x01\x02 not valid yaml !!! }")?;

    let content = fs::read_to_string(&path)?;
    let err =
        ModelRegistry::from_yaml(&content).expect_err("expected parse error for malformed YAML");

    assert!(
        matches!(err, ModelRegistryError::Parse(_)),
        "expected Parse error, got: {err}"
    );

    let _ = fs::remove_file(&path);
    Ok(())
}
