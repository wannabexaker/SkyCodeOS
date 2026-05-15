//! Phase 10A - GBNF grammar constraints.
//!
//! These tests verify that:
//! 1. The grammar file exists in the canonical location and parses as text.
//! 2. The task loop still succeeds when a repo includes the canonical grammar.

use std::fs;
use std::path::{Path, PathBuf};

use rusqlite::Connection;
use skycode_runtime::db::migrations::run_migrations;
use skycode_runtime::orchestrator::task_loop::{run_task_loop, TaskLoopInput};
use tempfile::TempDir;

const MOCK_RESPONSE: &str = r##"{
  "skycore_version": "0.1",
  "task_id": "phase10-task",
  "status": "ok",
  "summary": "Created HELLO.md",
  "artifacts": [
    {
      "kind": "rewrite",
      "id": "HELLO.md",
      "new_content": "hello world\n",
      "affected_files": ["HELLO.md"]
    }
  ],
  "tool_calls_requested": [],
  "requires_approval": true,
  "error": null
}"##;

mod phase10_grammar {
    use super::*;

    #[test]
    fn skycore_grammar_file_exists_at_canonical_path() -> Result<(), Box<dyn std::error::Error>> {
        let path = canonical_grammar_path();
        assert!(
            path.exists(),
            "agents/grammars/skycore.gbnf must exist at {}",
            path.display()
        );

        let content = fs::read_to_string(&path)?;
        assert!(content.contains("root ::= skycore-response"));
        assert!(content.contains("\\\"skycore_version\\\""));
        assert!(content.contains("\\\"rewrite\\\""));
        assert!(content.contains("\\\"diff\\\""));

        Ok(())
    }

    #[test]
    fn task_loop_uses_grammar_when_present() -> Result<(), Box<dyn std::error::Error>> {
        let temp = TempDir::new()?;
        let repo = temp.path().join("repo");
        fs::create_dir_all(&repo)?;

        write_agent_identity(&repo)?;
        write_model_registry(&repo)?;
        write_mock_model_response(&repo)?;
        copy_canonical_grammar(&repo)?;

        let conn = Connection::open(temp.path().join("phase10.db"))?;
        run_migrations(&conn, &migrations_dir())?;

        let input = TaskLoopInput {
            task_id: "phase10-task".to_string(),
            project_id: "phase10-project".to_string(),
            goal: "Create a HELLO.md file with hello world".to_string(),
            repo_root: repo.to_string_lossy().to_string(),
            profile: "precise".to_string(),
            allow_destructive: false,
        };

        let output = run_task_loop(&conn, &input)?;
        assert!(
            output.diff.diff_text.contains("HELLO.md"),
            "diff must reference HELLO.md"
        );
        assert!(
            output.diff.diff_text.contains("hello world"),
            "diff must include model-provided content"
        );

        Ok(())
    }
}

fn canonical_grammar_path() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("agents")
        .join("grammars")
        .join("skycore.gbnf")
}

fn migrations_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("memory")
        .join("migrations")
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
    let model_path = repo.join("mock-model.gguf");
    fs::write(&model_path, "")?;
    let model_path = model_path.to_string_lossy().replace('\\', "/");

    fs::write(
        repo.join("agents").join("models.yaml"),
        format!(
            "models:\n  - name: local-coder\n    runtime: local_gguf\n    executable: llama-server\n    path: {model_path}\n    ctx_size: 4096\n    gpu_layers: 0\n    strengths: [code_edit]\n    enabled: true\n    threads: 1\n    n_cpu_moe: ~\n    no_mmap: false\n    mlock: false\n    port: 19999\n"
        ),
    )?;
    Ok(())
}

fn write_mock_model_response(repo: &Path) -> Result<(), Box<dyn std::error::Error>> {
    let skycode_dir = repo.join(".skycode");
    fs::create_dir_all(&skycode_dir)?;
    fs::write(skycode_dir.join("mock_model_response.json"), MOCK_RESPONSE)?;
    Ok(())
}

fn copy_canonical_grammar(repo: &Path) -> Result<(), Box<dyn std::error::Error>> {
    let grammar_dst = repo.join("agents").join("grammars").join("skycore.gbnf");
    let parent = grammar_dst
        .parent()
        .ok_or("grammar destination has no parent")?;
    fs::create_dir_all(parent)?;
    fs::copy(canonical_grammar_path(), grammar_dst)?;
    Ok(())
}
