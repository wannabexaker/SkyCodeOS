//! Anti-regression end-to-end integration test.
//!
//! Verifies the full chain:
//!   run_task_loop -> mock SkyCore JSON -> diff -> ApprovalToken -> apply_diff -> file on disk
//!
//! Does not require a real GPU or llama.cpp process.
//! The model response is injected via .skycode/mock_model_response.json.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

use ring::rand::SystemRandom;
use ring::signature::Ed25519KeyPair;
use rusqlite::Connection;
use skycode_runtime::approval::token::{public_key_bytes, ApprovalToken};
use skycode_runtime::approval::validator::{register_signing_key, validate_token};
use skycode_runtime::db::migrations::run_migrations;
use skycode_runtime::orchestrator::task_loop::{run_task_loop, TaskLoopInput};
use skycode_runtime::tools::apply::apply_diff;
use tempfile::TempDir;

const MOCK_RESPONSE: &str = r##"{
  "skycore_version": "0.1",
  "task_id": "e2e-test-task",
  "status": "ok",
  "summary": "Created CHANGELOG.md",
  "artifacts": [
    {
      "kind": "rewrite",
      "id": "CHANGELOG.md",
      "new_content": "# Changelog\n\n## Unreleased\n",
      "affected_files": ["CHANGELOG.md"]
    }
  ],
  "tool_calls_requested": [],
  "requires_approval": true,
  "error": null
}"##;

mod anti_regression {
    use super::*;

    #[test]
    fn end_to_end_new_file_written_to_disk() -> Result<(), Box<dyn std::error::Error>> {
        let temp = TempDir::new()?;

        let repo = create_temp_git_repo(&temp)?;
        write_agent_identity(&repo)?;
        write_model_registry(&repo)?;
        write_mock_model_response(&repo)?;

        let db_path = temp.path().join("e2e.db");
        let conn = Connection::open(&db_path)?;
        run_migrations(&conn, &migrations_dir())?;

        let (key_pair, pub_key_bytes) = make_keypair()?;
        let pub_key_hex: String = pub_key_bytes.iter().map(|b| format!("{b:02x}")).collect();
        register_signing_key(&conn, "coder-primary", &pub_key_hex, unix_now()?)?;

        let input = TaskLoopInput {
            task_id: "e2e-test-task".to_string(),
            project_id: "e2e-project".to_string(),
            goal: "Add CHANGELOG.md".to_string(),
            repo_root: repo.to_string_lossy().to_string(),
            profile: "precise".to_string(),
            allow_destructive: false,
        };
        let output = run_task_loop(&conn, &input)?;

        assert!(!output.diff.diff_text.is_empty(), "diff must be non-empty");
        assert!(
            output.diff.diff_text.contains("CHANGELOG.md"),
            "diff must reference CHANGELOG.md"
        );

        let precheck_token = ApprovalToken::create_signed(
            "e2e-project",
            output.diff.id.to_string(),
            "coder-primary",
            "coder-primary",
            "e2e-precheck-nonce".to_string(),
            &key_pair,
        )?;

        let precheck = Connection::open(&db_path)?;
        run_migrations(&precheck, &migrations_dir())?;
        register_signing_key(&precheck, "coder-primary", &pub_key_hex, unix_now()?)?;
        validate_token(
            &precheck,
            &precheck_token,
            "e2e-project",
            &output.diff.id.to_string(),
            "coder-primary",
            "e2e-test-task",
        )?;

        let token = ApprovalToken::create_signed(
            "e2e-project",
            output.diff.id.to_string(),
            "coder-primary",
            "coder-primary",
            "e2e-apply-nonce".to_string(),
            &key_pair,
        )?;

        apply_diff(
            &conn,
            &token,
            "coder-primary",
            "e2e-test-task",
            &repo,
            "e2e-project",
            &output.diff,
        )?;

        let written = repo.join("CHANGELOG.md");
        assert!(written.exists(), "CHANGELOG.md must exist after apply_diff");
        let content = fs::read_to_string(&written)?;
        assert!(
            content.contains("Unreleased"),
            "CHANGELOG.md must contain 'Unreleased'"
        );

        Ok(())
    }
}

fn write_agent_identity(repo: &Path) -> Result<(), Box<dyn std::error::Error>> {
    let agents = repo.join("agents").join("coder-primary").join("core");
    fs::create_dir_all(&agents)?;
    fs::write(
        agents.join("soul.yaml"),
        "id: coder-primary\nname: Coder Primary\nrole: persistent_coder\ncore_values:\n  - correctness\n",
    )?;
    fs::write(
        agents.join("doctrine.yaml"),
        "must_never:\n  - write_without_approval\nmust_always:\n  - produce_diff_before_apply\napproval_required_for:\n  - file_write\n",
    )?;
    fs::write(
        agents.join("heart.yaml"),
        "communication_style: concise\nerror_handling: fail_visible\n",
    )?;
    fs::write(
        agents.join("mind.yaml"),
        "planning_depth: shallow_task_level\nrisk_tolerance: low\n",
    )?;
    Ok(())
}

fn write_model_registry(repo: &Path) -> Result<(), Box<dyn std::error::Error>> {
    fs::write(
        repo.join("agents").join("models.yaml"),
        "models:\n  - name: mock-model\n    runtime: local_gguf\n    executable: /dev/null\n    path: /dev/null\n    ctx_size: 4096\n    gpu_layers: 0\n    strengths: [code_edit]\n    enabled: true\n    threads: 1\n    n_cpu_moe: ~\n    no_mmap: false\n    mlock: false\n    port: 19999\n",
    )?;
    Ok(())
}

fn write_mock_model_response(repo: &Path) -> Result<(), Box<dyn std::error::Error>> {
    let skycode_dir = repo.join(".skycode");
    fs::create_dir_all(&skycode_dir)?;
    fs::write(skycode_dir.join("mock_model_response.json"), MOCK_RESPONSE)?;
    Ok(())
}

fn migrations_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("memory")
        .join("migrations")
}

fn make_keypair() -> Result<(Ed25519KeyPair, Vec<u8>), Box<dyn std::error::Error>> {
    let rng = SystemRandom::new();
    let pkcs8 =
        Ed25519KeyPair::generate_pkcs8(&rng).map_err(|_| "failed to generate Ed25519 key pair")?;
    let key_pair = Ed25519KeyPair::from_pkcs8(pkcs8.as_ref())
        .map_err(|_| "failed to parse Ed25519 key pair")?;
    let pub_bytes = public_key_bytes(&key_pair);
    Ok((key_pair, pub_bytes))
}

fn create_temp_git_repo(temp: &TempDir) -> Result<PathBuf, Box<dyn std::error::Error>> {
    let repo = temp.path().join("repo");
    fs::create_dir_all(&repo)?;
    run_git(&repo, &["init"])?;
    run_git(&repo, &["config", "core.autocrlf", "false"])?;
    run_git(&repo, &["config", "core.eol", "lf"])?;
    run_git(&repo, &["config", "user.email", "e2e@skycode.local"])?;
    run_git(&repo, &["config", "user.name", "e2e-test"])?;
    fs::write(repo.join(".gitkeep"), "")?;
    run_git(&repo, &["add", ".gitkeep"])?;
    run_git(&repo, &["commit", "-m", "init"])?;
    Ok(repo)
}

fn run_git(repo: &Path, args: &[&str]) -> Result<(), Box<dyn std::error::Error>> {
    let out = Command::new("git")
        .arg("-C")
        .arg(repo)
        .args(args)
        .output()?;
    if !out.status.success() {
        return Err(format!(
            "git {:?} failed: {}",
            args,
            String::from_utf8_lossy(&out.stderr)
        )
        .into());
    }
    Ok(())
}

fn unix_now() -> Result<i64, Box<dyn std::error::Error>> {
    Ok(i64::try_from(
        SystemTime::now().duration_since(UNIX_EPOCH)?.as_secs(),
    )?)
}
