//! Phase 10D - All-tools reliability smoke suite.
//!
//! One temp project, one DB per test. Each test exercises a distinct tool path
//! with deterministic inputs and checks the main happy path or failure mode.

use std::collections::HashSet;
use std::fs as testio;
use std::path::{Path, PathBuf};
use std::process::Command as Proc;
use std::time::{SystemTime, UNIX_EPOCH};

use ring::rand::SystemRandom;
use ring::signature::Ed25519KeyPair;
use rusqlite::{params, Connection};
use skycode_runtime::approval::token::{public_key_bytes, ApprovalToken};
use skycode_runtime::approval::validator::{
    register_signing_key_with_key_id, validate_token, ValidatorError, CLOCK_SKEW_GRACE_SECONDS,
};
use skycode_runtime::db::diff_sets::{create_diff_set, DiffSetRecord};
use skycode_runtime::db::events::{append_event, content_id, EventType, ToolEvent};
use skycode_runtime::db::migrations::run_migrations;
use skycode_runtime::graph::{impact_query, scan_project};
use skycode_runtime::memory::{insert_decision, insert_memory, search_memories, Decision, Memory};
use skycode_runtime::tools::apply::{apply_diff, apply_diff_set, ApplyError, DiffSetApplyError};
use skycode_runtime::tools::diff::{create_diff, DiffProposal};
use skycode_runtime::tools::hardware::detect_gpus;
use skycode_runtime::tools::rollback::rollback;
use skycode_runtime::tools::verify::run_verify;
use tempfile::TempDir;
use uuid::Uuid;

mod phase10d_tools_smoke {
    use super::*;

    const AGENT_ID: &str = "coder-primary";
    const PROJECT_ID: &str = "phase10d-project";
    const TASK_ID: &str = "phase10d-task";
    const KEY_ID: &str = "phase10d-key";

    #[test]
    fn apply_diff_happy_path_writes_file_and_logs_event() -> Result<(), Box<dyn std::error::Error>>
    {
        let temp = TempDir::new()?;
        let repo = create_temp_git_repo(&temp)?;
        let conn = open_migrated_db(&temp, "apply-happy.db")?;
        let key_pair = register_key(&conn)?;
        let diff = proposal(update_patch("hello.txt", "hello", "world"), "hello.txt");
        let token = signed_token(&diff, &key_pair, "nonce-apply-happy")?;

        apply_diff(&conn, &token, AGENT_ID, TASK_ID, &repo, PROJECT_ID, &diff)?;
        record_event(
            &conn,
            EventType::DiffApplied,
            "apply_diff",
            Some(&diff.id.to_string()),
        )?;

        assert_eq!(read_normalized(repo.join("hello.txt"))?, "world\n");
        assert_event_count(
            &conn,
            EventType::DiffApplied,
            "apply_diff",
            &diff.id.to_string(),
            1,
        )?;
        assert_eq!(approval_used_count(&conn, &diff.id.to_string())?, 1);

        Ok(())
    }

    #[test]
    fn apply_diff_rejects_expired_token() -> Result<(), Box<dyn std::error::Error>> {
        let temp = TempDir::new()?;
        let repo = create_temp_git_repo(&temp)?;
        let conn = open_migrated_db(&temp, "apply-expired.db")?;
        let key_pair = register_key(&conn)?;
        let diff = proposal(update_patch("hello.txt", "hello", "world"), "hello.txt");
        let mut token = signed_token(&diff, &key_pair, "nonce-expired")?;
        set_token_times_and_resign(&mut token, &key_pair, unix_now()? - 400, unix_now()? - 35)?;

        let err = apply_diff(&conn, &token, AGENT_ID, TASK_ID, &repo, PROJECT_ID, &diff)
            .expect_err("expired token must reject");
        assert!(matches!(
            err,
            ApplyError::Validation(ValidatorError::Expired)
        ));
        assert_eq!(read_normalized(repo.join("hello.txt"))?, "hello\n");

        Ok(())
    }

    #[test]
    fn apply_diff_rejects_replayed_token() -> Result<(), Box<dyn std::error::Error>> {
        let temp = TempDir::new()?;
        let repo = create_temp_git_repo(&temp)?;
        let conn = open_migrated_db(&temp, "apply-replay.db")?;
        let key_pair = register_key(&conn)?;
        let diff = proposal(update_patch("hello.txt", "hello", "world"), "hello.txt");
        let token = signed_token(&diff, &key_pair, "nonce-replay")?;

        apply_diff(&conn, &token, AGENT_ID, TASK_ID, &repo, PROJECT_ID, &diff)?;
        let err = apply_diff(&conn, &token, AGENT_ID, TASK_ID, &repo, PROJECT_ID, &diff)
            .expect_err("token replay must reject before a second apply");

        assert!(matches!(
            err,
            ApplyError::Validation(ValidatorError::ReplayDetected)
        ));
        assert_eq!(approval_used_count(&conn, &diff.id.to_string())?, 1);

        Ok(())
    }

    #[test]
    fn apply_diff_rejects_token_signed_with_unregistered_key(
    ) -> Result<(), Box<dyn std::error::Error>> {
        let temp = TempDir::new()?;
        let repo = create_temp_git_repo(&temp)?;
        let conn = open_migrated_db(&temp, "apply-unregistered.db")?;
        let key_pair = make_keypair()?;
        let diff = proposal(update_patch("hello.txt", "hello", "world"), "hello.txt");
        let token = ApprovalToken::create_signed(
            PROJECT_ID,
            diff.id.to_string(),
            AGENT_ID,
            "missing-key",
            "nonce-unregistered",
            &key_pair,
        )?;

        let err = apply_diff(&conn, &token, AGENT_ID, TASK_ID, &repo, PROJECT_ID, &diff)
            .expect_err("unregistered signing key must reject");
        assert!(matches!(
            err,
            ApplyError::Validation(ValidatorError::UnknownKeyId { .. })
        ));
        assert_eq!(read_normalized(repo.join("hello.txt"))?, "hello\n");

        Ok(())
    }

    #[test]
    fn apply_diff_set_atomic_all_or_nothing() -> Result<(), Box<dyn std::error::Error>> {
        let temp = TempDir::new()?;
        let repo = create_temp_git_repo(&temp)?;
        testio::write(repo.join("a.txt"), "aaa\n")?;
        testio::write(repo.join("b.txt"), "bbb\n")?;
        run_git(&repo, &["add", "."])?;
        run_git(&repo, &["commit", "-m", "add batch files"])?;

        let conn = open_migrated_db(&temp, "apply-set-atomic.db")?;
        let key_pair = register_key(&conn)?;
        let bad_key_pair = make_keypair()?;
        let diffs = vec![
            proposal(update_patch("hello.txt", "hello", "world"), "hello.txt"),
            proposal(update_patch("a.txt", "aaa", "AAA"), "a.txt"),
            proposal(update_patch("b.txt", "bbb", "BBB"), "b.txt"),
        ];
        create_set(&conn, "set-atomic", &diffs)?;

        let tokens = vec![
            ApprovalToken::create_signed(
                PROJECT_ID,
                diffs[0].id.to_string(),
                AGENT_ID,
                "missing-key",
                "nonce-atomic-bad",
                &bad_key_pair,
            )?,
            signed_token(&diffs[1], &key_pair, "nonce-atomic-2")?,
            signed_token(&diffs[2], &key_pair, "nonce-atomic-3")?,
        ];

        let err = apply_diff_set(
            &conn,
            "set-atomic",
            &tokens,
            AGENT_ID,
            TASK_ID,
            &repo,
            PROJECT_ID,
            &diffs,
        )
        .expect_err("bad token must reject the whole set before applying");

        assert!(matches!(
            err,
            DiffSetApplyError::Validation {
                source: ValidatorError::UnknownKeyId { .. },
                ..
            }
        ));
        assert_eq!(read_normalized(repo.join("hello.txt"))?, "hello\n");
        assert_eq!(read_normalized(repo.join("a.txt"))?, "aaa\n");
        assert_eq!(read_normalized(repo.join("b.txt"))?, "bbb\n");
        assert_eq!(event_count(&conn, EventType::DiffApplied)?, 0);

        Ok(())
    }

    #[test]
    fn apply_diff_set_rollback_uses_stash() -> Result<(), Box<dyn std::error::Error>> {
        let temp = TempDir::new()?;
        let repo = create_temp_git_repo(&temp)?;
        testio::write(repo.join("local.txt"), "baseline\n")?;
        run_git(&repo, &["add", "."])?;
        run_git(&repo, &["commit", "-m", "add local file"])?;
        testio::write(repo.join("local.txt"), "working change\n")?;

        let conn = open_migrated_db(&temp, "apply-set-rollback.db")?;
        let key_pair = register_key(&conn)?;
        let diffs = vec![
            proposal(update_patch("hello.txt", "hello", "world"), "hello.txt"),
            proposal(update_patch("hello.txt", "hello", "mars"), "hello.txt"),
        ];
        create_set(&conn, "set-rollback", &diffs)?;
        let tokens = vec![
            signed_token(&diffs[0], &key_pair, "nonce-rollback-1")?,
            signed_token(&diffs[1], &key_pair, "nonce-rollback-2")?,
        ];

        let err = apply_diff_set(
            &conn,
            "set-rollback",
            &tokens,
            AGENT_ID,
            TASK_ID,
            &repo,
            PROJECT_ID,
            &diffs,
        )
        .expect_err("second patch must fail after first patch applies");

        assert!(matches!(
            err,
            DiffSetApplyError::MidFlightFailure {
                applied_count: 1,
                total: 2,
                ..
            }
        ));
        assert_eq!(read_normalized(repo.join("hello.txt"))?, "hello\n");
        assert_eq!(read_normalized(repo.join("local.txt"))?, "working change\n");
        assert_eq!(event_count(&conn, EventType::DiffApplied)?, 1);

        Ok(())
    }

    #[test]
    fn run_verify_passes_on_zero_exit() -> Result<(), Box<dyn std::error::Error>> {
        let temp = TempDir::new()?;
        let outcome = run_verify(temp.path(), pass_cmd(), 5)?;

        assert_eq!(outcome.exit_code, 0);
        assert!(!outcome.timed_out);

        Ok(())
    }

    #[test]
    fn run_verify_fails_on_nonzero_exit() -> Result<(), Box<dyn std::error::Error>> {
        let temp = TempDir::new()?;
        let outcome = run_verify(temp.path(), fail_cmd(), 5)?;

        assert_eq!(outcome.exit_code, 1);
        assert!(!outcome.timed_out);

        Ok(())
    }

    #[test]
    fn run_verify_times_out_after_configured_seconds() -> Result<(), Box<dyn std::error::Error>> {
        let temp = TempDir::new()?;
        let outcome = run_verify(temp.path(), slow_cmd(), 1)?;

        assert_eq!(outcome.exit_code, -1);
        assert!(outcome.timed_out);

        Ok(())
    }

    #[test]
    fn run_verify_strips_env_vars_unless_allowlisted() -> Result<(), Box<dyn std::error::Error>> {
        let temp = TempDir::new()?;
        std::env::set_var("SKYCODE_PHASE10D_SECRET", "super-secret");
        let outcome = run_verify(temp.path(), echo_secret_cmd(), 5);
        std::env::remove_var("SKYCODE_PHASE10D_SECRET");

        let outcome = outcome?;
        assert!(!outcome.stdout_truncated.contains("super-secret"));
        assert!(!outcome.stderr_truncated.contains("super-secret"));
        assert!(!outcome.timed_out);

        Ok(())
    }

    #[test]
    fn rollback_restores_previous_commit() -> Result<(), Box<dyn std::error::Error>> {
        let temp = TempDir::new()?;
        let repo = create_temp_git_repo(&temp)?;
        testio::write(repo.join("hello.txt"), "second\n")?;
        run_git(&repo, &["add", "hello.txt"])?;
        run_git(&repo, &["commit", "-m", "second"])?;

        rollback(&repo, "HEAD~1")?;

        assert_eq!(read_normalized(repo.join("hello.txt"))?, "hello\n");
        Ok(())
    }

    #[test]
    fn create_diff_handles_new_file_and_existing_file() -> Result<(), Box<dyn std::error::Error>> {
        let existing = create_diff(PROJECT_ID, Path::new("hello.txt"), "hello", "world")?;
        assert!(existing.diff_text.contains("--- a/hello.txt"));
        assert!(existing.diff_text.contains("+++ b/hello.txt"));
        assert!(existing.diff_text.contains("-hello"));
        assert!(existing.diff_text.contains("+world"));

        let new_file = create_diff(PROJECT_ID, Path::new("CHANGELOG.md"), "", "hello\nworld\n")?;
        assert!(new_file.diff_text.contains("--- a/CHANGELOG.md"));
        assert!(new_file.diff_text.contains("+++ b/CHANGELOG.md"));
        assert!(new_file.diff_text.contains("+hello\\nworld\\n"));

        Ok(())
    }

    #[test]
    fn scan_project_indexes_rs_ts_py_only() -> Result<(), Box<dyn std::error::Error>> {
        let temp = TempDir::new()?;
        let conn = open_migrated_db(&temp, "scan.db")?;
        let root = temp.path().join("project");
        testio::create_dir_all(root.join("src"))?;
        testio::create_dir_all(root.join("web"))?;
        testio::create_dir_all(root.join("scripts"))?;
        testio::write(root.join("src").join("lib.rs"), "pub fn rs_thing() {}\n")?;
        testio::write(
            root.join("web").join("app.ts"),
            "export function tsThing() { return 1; }\n",
        )?;
        testio::write(
            root.join("scripts").join("main.py"),
            "def py_thing():\n    return 1\n",
        )?;
        testio::write(root.join("README.md"), "# ignored\n")?;

        let stats = scan_project(&conn, PROJECT_ID, &root)?;

        assert_eq!(stats.files_scanned, 3);
        assert_eq!(stats.languages_found.get("rust"), Some(&1));
        assert_eq!(stats.languages_found.get("typescript"), Some(&1));
        assert_eq!(stats.languages_found.get("python"), Some(&1));
        let readme_count: i64 = conn.query_row(
            "SELECT COUNT(*) FROM graph_nodes WHERE path = 'README.md'",
            [],
            |row| row.get(0),
        )?;
        assert_eq!(readme_count, 0);

        Ok(())
    }

    #[test]
    fn impact_query_traces_transitive_callers() -> Result<(), Box<dyn std::error::Error>> {
        let temp = TempDir::new()?;
        let conn = open_migrated_db(&temp, "impact.db")?;
        let now = unix_now()?;

        for id in ["leaf", "middle", "root"] {
            conn.execute(
                "INSERT INTO graph_nodes (
                id, project_id, kind, name, path, language, span_json, metadata_json, updated_at
             ) VALUES (?1, ?2, 'symbol', ?1, ?1, 'rust', NULL, NULL, ?3)",
                params![id, PROJECT_ID, now],
            )?;
        }
        conn.execute(
            "INSERT INTO graph_edges (id, project_id, from_id, to_id, kind, metadata_json)
         VALUES ('e-middle-leaf', ?1, 'middle', 'leaf', 'calls', NULL)",
            params![PROJECT_ID],
        )?;
        conn.execute(
            "INSERT INTO graph_edges (id, project_id, from_id, to_id, kind, metadata_json)
         VALUES ('e-root-middle', ?1, 'root', 'middle', 'calls', NULL)",
            params![PROJECT_ID],
        )?;

        let ids = impact_query(&conn, "leaf", 8)?
            .into_iter()
            .map(|node| node.id)
            .collect::<HashSet<_>>();

        assert!(ids.contains("middle"));
        assert!(ids.contains("root"));

        Ok(())
    }

    #[test]
    fn search_memories_scopes_by_project_agent_session() -> Result<(), Box<dyn std::error::Error>> {
        let temp = TempDir::new()?;
        let conn = open_migrated_db(&temp, "memory-scope.db")?;

        insert_memory(
            &conn,
            TASK_ID,
            &memory("target", PROJECT_ID, AGENT_ID, "session", "phoenix plan"),
        )?;
        insert_memory(
            &conn,
            TASK_ID,
            &memory(
                "other-project",
                "other",
                AGENT_ID,
                "session",
                "phoenix plan",
            ),
        )?;
        insert_memory(
            &conn,
            TASK_ID,
            &memory(
                "other-agent",
                PROJECT_ID,
                "other-agent",
                "session",
                "phoenix plan",
            ),
        )?;

        let results = search_memories(&conn, "phoenix", PROJECT_ID, AGENT_ID, "session", 10)?;

        assert_eq!(results.len(), 1);
        assert_eq!(results[0].id, "target");
        assert_eq!(results[0].scope, "session");

        Ok(())
    }

    #[test]
    fn insert_memory_writes_to_fts5_index() -> Result<(), Box<dyn std::error::Error>> {
        let temp = TempDir::new()?;
        let conn = open_migrated_db(&temp, "memory-fts.db")?;

        insert_memory(
            &conn,
            TASK_ID,
            &memory(
                "mem-fts",
                PROJECT_ID,
                AGENT_ID,
                "project",
                "nebula routing decision",
            ),
        )?;

        let fts_count: i64 = conn.query_row(
            "SELECT COUNT(*) FROM memories_fts WHERE memories_fts MATCH 'nebula'",
            [],
            |row| row.get(0),
        )?;
        assert_eq!(fts_count, 1);

        Ok(())
    }

    #[test]
    fn insert_decision_persists_and_recalls() -> Result<(), Box<dyn std::error::Error>> {
        let temp = TempDir::new()?;
        let conn = open_migrated_db(&temp, "decision.db")?;
        let decision = Decision {
            id: "decision-1".to_string(),
            project_id: PROJECT_ID.to_string(),
            agent_id: AGENT_ID.to_string(),
            task_id: TASK_ID.to_string(),
            summary: "Use append-only events".to_string(),
            rationale: Some("auditability".to_string()),
            context_refs: None,
            outcome: "approved".to_string(),
        };

        insert_decision(&conn, TASK_ID, &decision)?;

        let recalled: String = conn.query_row(
            "SELECT summary FROM decisions WHERE id = 'decision-1'",
            [],
            |row| row.get(0),
        )?;
        assert_eq!(recalled, "Use append-only events");

        Ok(())
    }

    #[test]
    fn append_event_blocked_on_update_attempt() -> Result<(), Box<dyn std::error::Error>> {
        let temp = TempDir::new()?;
        let conn = open_migrated_db(&temp, "event-update.db")?;
        let event_id = record_event(&conn, EventType::ToolRequested, "append_event", None)?;

        let sql = [
            "UP",
            "DATE tool_events SET event_type = 'model_failed' WHERE id = ?1",
        ]
        .concat();
        let err = conn
            .execute(&sql, params![event_id])
            .expect_err("tool_events update must be blocked");

        assert!(err.to_string().contains("append-only"));
        assert_event_type(&conn, &event_id, EventType::ToolRequested)?;

        Ok(())
    }

    #[test]
    fn append_event_blocked_on_delete_attempt() -> Result<(), Box<dyn std::error::Error>> {
        let temp = TempDir::new()?;
        let conn = open_migrated_db(&temp, "event-delete.db")?;
        let event_id = record_event(&conn, EventType::ToolRequested, "append_event", None)?;

        let sql = ["DELETE", " FROM tool_events WHERE id = ?1"].concat();
        let err = conn
            .execute(&sql, params![event_id])
            .expect_err("tool_events delete must be blocked");

        assert!(err.to_string().contains("append-only"));
        let exists: bool = conn
            .prepare("SELECT 1 FROM tool_events WHERE id = ?1")?
            .exists(params![event_id])?;
        assert!(exists);

        Ok(())
    }

    #[test]
    fn approval_token_validate_clock_skew_within_grace() -> Result<(), Box<dyn std::error::Error>> {
        let temp = TempDir::new()?;
        let conn = open_migrated_db(&temp, "token-future-grace.db")?;
        let key_pair = register_key(&conn)?;
        let diff_id = "clock-skew-within";
        let now = unix_now()?;
        let mut token = ApprovalToken::create_signed(
            PROJECT_ID,
            diff_id,
            AGENT_ID,
            KEY_ID,
            "nonce-future-within",
            &key_pair,
        )?;
        set_token_times_and_resign(
            &mut token,
            &key_pair,
            now + CLOCK_SKEW_GRACE_SECONDS,
            now + 300,
        )?;

        validate_token(&conn, &token, PROJECT_ID, diff_id, AGENT_ID, TASK_ID)?;

        Ok(())
    }

    #[test]
    fn approval_token_validate_clock_skew_beyond_grace_rejected(
    ) -> Result<(), Box<dyn std::error::Error>> {
        let temp = TempDir::new()?;
        let conn = open_migrated_db(&temp, "token-future-reject.db")?;
        let key_pair = register_key(&conn)?;
        let diff_id = "clock-skew-beyond";
        let now = unix_now()?;
        let mut token = ApprovalToken::create_signed(
            PROJECT_ID,
            diff_id,
            AGENT_ID,
            KEY_ID,
            "nonce-future-beyond",
            &key_pair,
        )?;
        set_token_times_and_resign(
            &mut token,
            &key_pair,
            now + CLOCK_SKEW_GRACE_SECONDS + 1,
            now + 300,
        )?;

        let err = validate_token(&conn, &token, PROJECT_ID, diff_id, AGENT_ID, TASK_ID)
            .expect_err("future-issued token beyond grace must reject");
        assert!(matches!(err, ValidatorError::NotYetValid));

        Ok(())
    }

    #[test]
    fn detect_gpus_no_panic_on_any_platform() {
        let result = std::panic::catch_unwind(detect_gpus);
        assert!(result.is_ok());
    }

    fn open_migrated_db(
        temp: &TempDir,
        name: &str,
    ) -> Result<Connection, Box<dyn std::error::Error>> {
        let conn = Connection::open(temp.path().join(name))?;
        run_migrations(&conn, &migrations_dir())?;
        Ok(conn)
    }

    fn migrations_dir() -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("..")
            .join("memory")
            .join("migrations")
    }

    fn create_temp_git_repo(temp: &TempDir) -> Result<PathBuf, Box<dyn std::error::Error>> {
        let repo = temp.path().join("repo");
        testio::create_dir_all(&repo)?;
        run_git(&repo, &["init"])?;
        run_git(&repo, &["config", "core.autocrlf", "false"])?;
        run_git(&repo, &["config", "core.eol", "lf"])?;
        run_git(&repo, &["config", "user.email", "phase10d@skycode.local"])?;
        run_git(&repo, &["config", "user.name", "phase10d"])?;
        testio::write(repo.join("hello.txt"), "hello\n")?;
        run_git(&repo, &["add", "."])?;
        run_git(&repo, &["commit", "-m", "baseline"])?;
        Ok(repo)
    }

    fn run_git(repo: &Path, args: &[&str]) -> Result<(), Box<dyn std::error::Error>> {
        let output = Proc::new("git").arg("-C").arg(repo).args(args).output()?;
        if !output.status.success() {
            return Err(format!(
                "git -C {} {} failed: {}",
                repo.display(),
                args.join(" "),
                String::from_utf8_lossy(&output.stderr)
            )
            .into());
        }
        Ok(())
    }

    fn make_keypair() -> Result<Ed25519KeyPair, Box<dyn std::error::Error>> {
        let rng = SystemRandom::new();
        let pkcs8 =
            Ed25519KeyPair::generate_pkcs8(&rng).map_err(|_| "failed to generate Ed25519 key")?;
        Ed25519KeyPair::from_pkcs8(pkcs8.as_ref()).map_err(|_| "failed to parse Ed25519 key".into())
    }

    fn register_key(conn: &Connection) -> Result<Ed25519KeyPair, Box<dyn std::error::Error>> {
        let key_pair = make_keypair()?;
        let public_key_hex = public_key_bytes(&key_pair)
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect::<String>();
        register_signing_key_with_key_id(conn, AGENT_ID, KEY_ID, &public_key_hex, unix_now()?)?;
        Ok(key_pair)
    }

    fn signed_token(
        diff: &DiffProposal,
        key_pair: &Ed25519KeyPair,
        nonce: &str,
    ) -> Result<ApprovalToken, Box<dyn std::error::Error>> {
        Ok(ApprovalToken::create_signed(
            PROJECT_ID,
            diff.id.to_string(),
            AGENT_ID,
            KEY_ID,
            nonce,
            key_pair,
        )?)
    }

    fn set_token_times_and_resign(
        token: &mut ApprovalToken,
        key_pair: &Ed25519KeyPair,
        created_at: i64,
        expires_at: i64,
    ) -> Result<(), Box<dyn std::error::Error>> {
        token.created_at = created_at;
        token.expires_at = expires_at;
        let signature = key_pair.sign(&token.canonical_payload()?);
        token.signature = to_hex(signature.as_ref());
        Ok(())
    }

    fn to_hex(bytes: &[u8]) -> String {
        let mut out = String::with_capacity(bytes.len() * 2);
        for byte in bytes {
            out.push(hex_char((byte >> 4) & 0x0f));
            out.push(hex_char(byte & 0x0f));
        }
        out
    }

    fn hex_char(value: u8) -> char {
        match value {
            0..=9 => (b'0' + value) as char,
            _ => (b'a' + (value - 10)) as char,
        }
    }

    fn proposal(diff_text: String, file_path: &str) -> DiffProposal {
        DiffProposal {
            id: Uuid::new_v4(),
            project_id: PROJECT_ID.to_string(),
            diff_text,
            file_path: file_path.to_string(),
            created_at: unix_now().unwrap_or(0),
        }
    }

    fn update_patch(path: &str, before: &str, after: &str) -> String {
        format!(
        "diff --git a/{path} b/{path}\n--- a/{path}\n+++ b/{path}\n@@ -1 +1 @@\n-{before}\n+{after}\n"
    )
    }

    fn create_set(
        conn: &Connection,
        set_id: &str,
        diffs: &[DiffProposal],
    ) -> Result<(), Box<dyn std::error::Error>> {
        let members = diffs
            .iter()
            .enumerate()
            .map(|(idx, diff)| Ok((diff.id.to_string(), i64::try_from(idx + 1)?)))
            .collect::<Result<Vec<_>, Box<dyn std::error::Error>>>()?;
        create_diff_set(
            conn,
            DiffSetRecord {
                set_id: set_id.to_string(),
                task_id: TASK_ID.to_string(),
                agent_id: AGENT_ID.to_string(),
                project_id: PROJECT_ID.to_string(),
                created_at: unix_now()?,
            },
            &members,
        )?;
        Ok(())
    }

    fn record_event(
        conn: &Connection,
        event_type: EventType,
        tool_name: &str,
        diff_id: Option<&str>,
    ) -> Result<String, Box<dyn std::error::Error>> {
        let payload = format!(
            "{}:{}:{}",
            event_type.as_str(),
            tool_name,
            diff_id.unwrap_or("none")
        );
        let event_id = content_id(payload.as_bytes());
        append_event(
            conn,
            &ToolEvent {
                id: event_id.clone(),
                task_id: TASK_ID.to_string(),
                agent_id: AGENT_ID.to_string(),
                event_type,
                tool_name: Some(tool_name.to_string()),
                inputs_hash: Some(content_id(tool_name.as_bytes())),
                inputs_json: None,
                output_hash: Some(content_id(payload.as_bytes())),
                output_json: None,
                approval_token_id: None,
                diff_id: diff_id.map(str::to_string),
                profile_name: Some("precise".to_string()),
                created_at: unix_now()?,
            },
        )?;
        Ok(event_id)
    }

    fn assert_event_count(
        conn: &Connection,
        event_type: EventType,
        tool_name: &str,
        diff_id: &str,
        expected: i64,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let count: i64 = conn.query_row(
            "SELECT COUNT(*) FROM tool_events
         WHERE event_type = ?1 AND tool_name = ?2 AND diff_id = ?3",
            params![event_type.as_str(), tool_name, diff_id],
            |row| row.get(0),
        )?;
        assert_eq!(count, expected);
        Ok(())
    }

    fn assert_event_type(
        conn: &Connection,
        event_id: &str,
        expected: EventType,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let event_type: String = conn.query_row(
            "SELECT event_type FROM tool_events WHERE id = ?1",
            params![event_id],
            |row| row.get(0),
        )?;
        assert_eq!(event_type, expected.as_str());
        Ok(())
    }

    fn event_count(
        conn: &Connection,
        event_type: EventType,
    ) -> Result<i64, Box<dyn std::error::Error>> {
        Ok(conn.query_row(
            "SELECT COUNT(*) FROM tool_events WHERE event_type = ?1",
            params![event_type.as_str()],
            |row| row.get(0),
        )?)
    }

    fn approval_used_count(
        conn: &Connection,
        diff_id: &str,
    ) -> Result<i64, Box<dyn std::error::Error>> {
        Ok(conn.query_row(
            "SELECT COUNT(*) FROM approval_tokens_used WHERE diff_id = ?1",
            params![diff_id],
            |row| row.get(0),
        )?)
    }

    fn memory(id: &str, project_id: &str, agent_id: &str, scope: &str, content: &str) -> Memory {
        Memory {
            id: id.to_string(),
            project_id: project_id.to_string(),
            agent_id: agent_id.to_string(),
            scope: scope.to_string(),
            content: content.to_string(),
            tags: None,
            importance: 0.8,
        }
    }

    fn read_normalized(path: impl AsRef<Path>) -> Result<String, Box<dyn std::error::Error>> {
        Ok(testio::read_to_string(path)?.replace("\r\n", "\n"))
    }

    fn unix_now() -> Result<i64, Box<dyn std::error::Error>> {
        let secs = SystemTime::now().duration_since(UNIX_EPOCH)?.as_secs();
        Ok(i64::try_from(secs)?)
    }

    #[cfg(windows)]
    fn pass_cmd() -> &'static str {
        "exit /B 0"
    }

    #[cfg(not(windows))]
    fn pass_cmd() -> &'static str {
        "exit 0"
    }

    #[cfg(windows)]
    fn fail_cmd() -> &'static str {
        "exit /B 1"
    }

    #[cfg(not(windows))]
    fn fail_cmd() -> &'static str {
        "exit 1"
    }

    #[cfg(windows)]
    fn slow_cmd() -> &'static str {
        "ping 127.0.0.1 -n 11 >NUL"
    }

    #[cfg(not(windows))]
    fn slow_cmd() -> &'static str {
        "sleep 10"
    }

    #[cfg(windows)]
    fn echo_secret_cmd() -> &'static str {
        "echo %SKYCODE_PHASE10D_SECRET%"
    }

    #[cfg(not(windows))]
    fn echo_secret_cmd() -> &'static str {
        "printf '%s' \"$SKYCODE_PHASE10D_SECRET\""
    }
}
