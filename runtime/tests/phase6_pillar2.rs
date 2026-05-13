use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

use ring::rand::SystemRandom;
use ring::signature::{Ed25519KeyPair, KeyPair};
use rusqlite::{params, Connection};
use skycode_runtime::approval::token::{public_key_bytes, ApprovalToken};
use skycode_runtime::approval::validator::{register_signing_key, validate_token, ValidatorError};
use skycode_runtime::db::migrations::run_migrations;
use skycode_runtime::db::{create_diff_set, get_diff_set_members, DiffSetRecord};

#[test]
fn phase6_multifile_membership_immutable() -> Result<(), Box<dyn std::error::Error>> {
    let conn = migrated_conn()?;
    let record = diff_set("s1");

    create_diff_set(&conn, &record, &[("d1".to_string(), 1)])?;

    let err = conn
        .execute(
            "INSERT INTO diff_set_members (set_id, diff_id, ord) VALUES (?1, ?2, ?3)",
            params!["s1", "d2", 2_i64],
        )
        .expect_err("direct membership insert after set creation must fail");

    assert!(
        err.to_string().contains("append-only"),
        "expected append-only trigger error, got {err}"
    );

    Ok(())
}

#[test]
fn phase6_multifile_apply_tokens_per_diff() -> Result<(), Box<dyn std::error::Error>> {
    let conn = migrated_conn()?;
    let agent_id = "coder-primary";
    let task_id = "task-1";
    let key_pair = test_key_pair()?;
    let public_key_hex = public_key_bytes(&key_pair)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect::<String>();

    register_signing_key(&conn, agent_id, &public_key_hex, unix_now()?)?;

    create_diff_set(
        &conn,
        &diff_set("s1"),
        &[
            ("d1".to_string(), 1),
            ("d2".to_string(), 2),
            ("d3".to_string(), 3),
        ],
    )?;

    let token_d1 =
        ApprovalToken::create_signed("default", "d1", agent_id, agent_id, "nonce-d1", &key_pair)?;
    let token_d2 =
        ApprovalToken::create_signed("default", "d2", agent_id, agent_id, "nonce-d2", &key_pair)?;
    let token_d3 =
        ApprovalToken::create_signed("default", "d3", agent_id, agent_id, "nonce-d3", &key_pair)?;

    validate_token(&conn, &token_d1, "default", "d1", agent_id, task_id)?;
    validate_token(&conn, &token_d2, "default", "d2", agent_id, task_id)?;
    validate_token(&conn, &token_d3, "default", "d3", agent_id, task_id)?;

    let mismatch = validate_token(&conn, &token_d1, "default", "d2", agent_id, task_id)
        .expect_err("a token for d1 must not validate for d2");
    assert!(matches!(mismatch, ValidatorError::DiffBindingMismatch));

    Ok(())
}

#[test]
fn phase6_multifile_members_ordered() -> Result<(), Box<dyn std::error::Error>> {
    let conn = migrated_conn()?;

    create_diff_set(
        &conn,
        &diff_set("s1"),
        &[
            ("d1".to_string(), 20),
            ("d2".to_string(), 10),
            ("d3".to_string(), 15),
        ],
    )?;

    let members = get_diff_set_members(&conn, "s1")?;
    let ordered_diff_ids = members
        .iter()
        .map(|member| member.diff_id.as_str())
        .collect::<Vec<_>>();

    assert_eq!(ordered_diff_ids, vec!["d2", "d3", "d1"]);

    Ok(())
}

fn migrated_conn() -> Result<Connection, Box<dyn std::error::Error>> {
    let conn = Connection::open_in_memory()?;
    let migrations_dir = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("memory")
        .join("migrations");
    run_migrations(&conn, &migrations_dir)?;
    Ok(conn)
}

fn diff_set(set_id: &str) -> DiffSetRecord {
    DiffSetRecord {
        set_id: set_id.to_string(),
        task_id: "task-1".to_string(),
        agent_id: "coder-primary".to_string(),
        project_id: "default".to_string(),
        created_at: 1,
    }
}

fn test_key_pair() -> Result<Ed25519KeyPair, Box<dyn std::error::Error>> {
    let rng = SystemRandom::new();
    let pkcs8 =
        Ed25519KeyPair::generate_pkcs8(&rng).map_err(|_| "failed to generate Ed25519 key")?;
    Ed25519KeyPair::from_pkcs8(pkcs8.as_ref()).map_err(|_| "failed to parse Ed25519 key".into())
}

fn unix_now() -> Result<i64, Box<dyn std::error::Error>> {
    let secs = SystemTime::now().duration_since(UNIX_EPOCH)?.as_secs();
    Ok(i64::try_from(secs)?)
}

#[test]
fn phase6_multifile_atomic() -> Result<(), Box<dyn std::error::Error>> {
    // Real temp git repo: git apply works on disk, not in-memory.
    let repo = tempfile::tempdir()?;
    let run_git = |args: &[&str]| {
        std::process::Command::new("git")
            .args(args)
            .current_dir(repo.path())
            .output()
            .unwrap()
    };
    run_git(&["init"]);
    run_git(&["config", "user.email", "test@test.com"]);
    run_git(&["config", "user.name", "Test"]);
    std::fs::write(repo.path().join("hello.txt"), "hello\n")?;
    run_git(&["add", "."]);
    run_git(&["commit", "-m", "init"]);

    // A valid patch that applies cleanly.
    let valid_patch = concat!(
        "diff --git a/hello.txt b/hello.txt\n",
        "index ce01362..cc628cc 100644\n",
        "--- a/hello.txt\n",
        "+++ b/hello.txt\n",
        "@@ -1 +1 @@\n",
        "-hello\n",
        "+world\n",
    );

    // A broken patch referencing a file that does not exist.
    let broken_patch = concat!(
        "diff --git a/ghost.txt b/ghost.txt\n",
        "--- a/ghost.txt\n",
        "+++ b/ghost.txt\n",
        "@@ -1 +1 @@\n",
        "-old\n",
        "+new\n",
    );

    let conn = migrated_conn()?;
    let d1 = uuid::Uuid::new_v4();
    let d2 = uuid::Uuid::new_v4();

    let diffs = vec![
        skycode_runtime::tools::diff::DiffProposal {
            id: d1,
            project_id: "test-project".to_string(),
            diff_text: valid_patch.to_string(),
            file_path: "hello.txt".to_string(),
            created_at: now(),
        },
        skycode_runtime::tools::diff::DiffProposal {
            id: d2,
            project_id: "test-project".to_string(),
            diff_text: broken_patch.to_string(),
            file_path: "ghost.txt".to_string(),
            created_at: now(),
        },
    ];

    // Create a diff set with both members.
    skycode_runtime::db::create_diff_set(
        &conn,
        &skycode_runtime::db::DiffSetRecord {
            set_id: "set-atomic".to_string(),
            task_id: "task-1".to_string(),
            agent_id: "coder-primary".to_string(),
            project_id: "default".to_string(),
            created_at: now(),
        },
        &[(d1.to_string(), 1_i64), (d2.to_string(), 2_i64)],
    )?;

    // Phase 1 (precheck) must reject the broken patch before anything is applied.
    // Tokens slice is empty — we expect PrecheckFailed, not MissingToken.
    let result = skycode_runtime::tools::apply::apply_diff_set(
        &conn,
        "set-atomic",
        &[],
        "coder-primary",
        "task-1",
        repo.path(),
        "test-project",
        &diffs,
    );

    assert!(
        matches!(
            result,
            Err(skycode_runtime::tools::apply::DiffSetApplyError::PrecheckFailed { .. })
        ),
        "expected PrecheckFailed, got: {result:?}"
    );

    // The repo must be completely unchanged: hello.txt still reads "hello\n".
    let content = std::fs::read_to_string(repo.path().join("hello.txt"))?;
    assert_eq!(
        content, "hello\n",
        "repo must be unmodified after precheck rejection"
    );

    Ok(())
}

fn now() -> i64 {
    match SystemTime::now().duration_since(UNIX_EPOCH) {
        Ok(duration) => i64::try_from(duration.as_secs()).unwrap_or(i64::MAX),
        Err(_) => 0,
    }
}

/// ApprovalToken minted for project-a must be rejected when presented
/// for project-b — even if diff_id, agent_id, and signature are all valid.
#[test]
fn phase6_multifile_cross_project_tamper() {
    use ring::rand::SystemRandom;
    use ring::signature::Ed25519KeyPair;
    use skycode_runtime::approval::token::ApprovalToken;
    use skycode_runtime::approval::validator::{
        register_signing_key, validate_token, ValidatorError,
    };

    let conn = migrated_conn().expect("failed to create test DB");

    // Generate a key pair and register it
    let rng = SystemRandom::new();
    let pkcs8 = Ed25519KeyPair::generate_pkcs8(&rng).expect("failed to generate Ed25519 key");
    let key_pair = Ed25519KeyPair::from_pkcs8(pkcs8.as_ref()).expect("failed to parse key");
    let pub_hex: String = key_pair
        .public_key()
        .as_ref()
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect();
    register_signing_key(
        &conn,
        "agent-x",
        &pub_hex,
        unix_now().expect("failed to get time"),
    )
    .expect("failed to register key");

    // Mint token for project-a
    let token = ApprovalToken::create_signed(
        "project-a",
        "diff-x",
        "agent-x",
        "agent-x",
        "nonce-test",
        &key_pair,
    )
    .expect("failed to create token");

    // Present it for project-b — must be rejected
    let result = validate_token(
        &conn,
        &token,
        "project-b", // expected_project_id
        "diff-x",
        "agent-x",
        "task-1",
    );

    assert!(
        matches!(result, Err(ValidatorError::ProjectBindingMismatch)),
        "expected ProjectBindingMismatch, got: {result:?}"
    );
}

/// Test the mid-flight failure recovery path: when apply_diff_set processes
/// a set of diffs and one fails after others have been applied, the repo
/// must be recovered to its pre-attempt state (including uncommitted changes).
/// This exercises the git stash / git stash pop recovery in apply_diff_set Phase 4.
#[test]
fn phase6_multifile_rollback_real_repo() -> Result<(), Box<dyn std::error::Error>> {
    use std::fs;

    // Create real temp git repo
    let repo = tempfile::tempdir()?;
    let run_git = |args: &[&str]| {
        std::process::Command::new("git")
            .args(args)
            .current_dir(repo.path())
            .output()
            .unwrap()
    };

    // Initialize repo with 5 files
    run_git(&["init"]);
    run_git(&["config", "user.email", "test@test.com"]);
    run_git(&["config", "user.name", "Test"]);

    fs::write(repo.path().join("a.txt"), "aaa\n")?;
    fs::write(repo.path().join("b.txt"), "bbb\n")?;
    fs::write(repo.path().join("c.txt"), "ccc\n")?;
    fs::write(repo.path().join("d.txt"), "ddd\n")?;
    fs::write(repo.path().join("e.txt"), "eee\n")?;

    run_git(&["add", "."]);
    run_git(&["commit", "-m", "init"]);

    // Add uncommitted working-tree change (will be stashed and restored)
    fs::write(repo.path().join("f.txt"), "working-change\n")?;

    // Setup DB, key pair, and agent
    let conn = migrated_conn()?;
    let agent_id = "coder-primary";
    let task_id = "task-rollback";
    let project_id = "test-project";
    let key_pair = test_key_pair()?;
    let public_key_hex = public_key_bytes(&key_pair)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect::<String>();

    register_signing_key(&conn, agent_id, &public_key_hex, unix_now()?)?;

    // Create 5 diffs: first 4 valid, 5th causes mid-flight failure after 4 applied
    let d1_id = uuid::Uuid::new_v4();
    let d2_id = uuid::Uuid::new_v4();
    let d3_id = uuid::Uuid::new_v4();
    let d4_id = uuid::Uuid::new_v4();
    let d5_id = uuid::Uuid::new_v4();

    // d1: a.txt "aaa\n" → "AAA\n"
    let d1_patch = concat!(
        "diff --git a/a.txt b/a.txt\n",
        "--- a/a.txt\n",
        "+++ b/a.txt\n",
        "@@ -1 +1 @@\n",
        "-aaa\n",
        "+AAA\n",
    );

    // d2: b.txt "bbb\n" → "BBB\n"
    let d2_patch = concat!(
        "diff --git a/b.txt b/b.txt\n",
        "--- a/b.txt\n",
        "+++ b/b.txt\n",
        "@@ -1 +1 @@\n",
        "-bbb\n",
        "+BBB\n",
    );

    // d3: c.txt "ccc\n" → "CCC\n"
    let d3_patch = concat!(
        "diff --git a/c.txt b/c.txt\n",
        "--- a/c.txt\n",
        "+++ b/c.txt\n",
        "@@ -1 +1 @@\n",
        "-ccc\n",
        "+CCC\n",
    );

    // d4: d.txt "ddd\n" → "DDD\n"
    let d4_patch = concat!(
        "diff --git a/d.txt b/d.txt\n",
        "--- a/d.txt\n",
        "+++ b/d.txt\n",
        "@@ -1 +1 @@\n",
        "-ddd\n",
        "+DDD\n",
    );

    // d5: a.txt "aaa\n" → "zzz\n" — passes precheck but fails after d1 applied
    // (because a.txt will be "AAA\n" by the time d5 is applied)
    let d5_patch = concat!(
        "diff --git a/a.txt b/a.txt\n",
        "--- a/a.txt\n",
        "+++ b/a.txt\n",
        "@@ -1 +1 @@\n",
        "-aaa\n",
        "+zzz\n",
    );

    let diffs = vec![
        skycode_runtime::tools::diff::DiffProposal {
            id: d1_id,
            project_id: project_id.to_string(),
            diff_text: d1_patch.to_string(),
            file_path: "a.txt".to_string(),
            created_at: now(),
        },
        skycode_runtime::tools::diff::DiffProposal {
            id: d2_id,
            project_id: project_id.to_string(),
            diff_text: d2_patch.to_string(),
            file_path: "b.txt".to_string(),
            created_at: now(),
        },
        skycode_runtime::tools::diff::DiffProposal {
            id: d3_id,
            project_id: project_id.to_string(),
            diff_text: d3_patch.to_string(),
            file_path: "c.txt".to_string(),
            created_at: now(),
        },
        skycode_runtime::tools::diff::DiffProposal {
            id: d4_id,
            project_id: project_id.to_string(),
            diff_text: d4_patch.to_string(),
            file_path: "d.txt".to_string(),
            created_at: now(),
        },
        skycode_runtime::tools::diff::DiffProposal {
            id: d5_id,
            project_id: project_id.to_string(),
            diff_text: d5_patch.to_string(),
            file_path: "a.txt".to_string(),
            created_at: now(),
        },
    ];

    // Create diff set with all 5 members in order
    skycode_runtime::db::create_diff_set(
        &conn,
        &skycode_runtime::db::DiffSetRecord {
            set_id: "set-rollback".to_string(),
            task_id: task_id.to_string(),
            agent_id: agent_id.to_string(),
            project_id: project_id.to_string(),
            created_at: now(),
        },
        &[
            (d1_id.to_string(), 1_i64),
            (d2_id.to_string(), 2_i64),
            (d3_id.to_string(), 3_i64),
            (d4_id.to_string(), 4_i64),
            (d5_id.to_string(), 5_i64),
        ],
    )?;

    // Create approval tokens for all 5 diffs
    let token_d1 = ApprovalToken::create_signed(
        project_id,
        d1_id.to_string(),
        agent_id,
        agent_id,
        "nonce-d1",
        &key_pair,
    )?;
    let token_d2 = ApprovalToken::create_signed(
        project_id,
        d2_id.to_string(),
        agent_id,
        agent_id,
        "nonce-d2",
        &key_pair,
    )?;
    let token_d3 = ApprovalToken::create_signed(
        project_id,
        d3_id.to_string(),
        agent_id,
        agent_id,
        "nonce-d3",
        &key_pair,
    )?;
    let token_d4 = ApprovalToken::create_signed(
        project_id,
        d4_id.to_string(),
        agent_id,
        agent_id,
        "nonce-d4",
        &key_pair,
    )?;
    let token_d5 = ApprovalToken::create_signed(
        project_id,
        d5_id.to_string(),
        agent_id,
        agent_id,
        "nonce-d5",
        &key_pair,
    )?;

    let tokens = vec![token_d1, token_d2, token_d3, token_d4, token_d5];

    // Call apply_diff_set — should fail with MidFlightFailure after 4 diffs applied
    let result = skycode_runtime::tools::apply::apply_diff_set(
        &conn,
        "set-rollback",
        &tokens,
        agent_id,
        task_id,
        repo.path(),
        project_id,
        &diffs,
    );

    // Assert the expected failure: MidFlightFailure with applied_count=4, total=5
    assert!(
        matches!(
            result,
            Err(
                skycode_runtime::tools::apply::DiffSetApplyError::MidFlightFailure {
                    applied_count: 4,
                    total: 5,
                    ..
                }
            )
        ),
        "expected MidFlightFailure with applied_count=4, total=5, got: {result:?}"
    );

    // All 5 original files must be restored to their original content
    // (note: Windows git repos use \r\n line endings, so normalize for comparison)
    let normalize_newlines = |s: String| s.replace("\r\n", "\n");

    assert_eq!(
        normalize_newlines(fs::read_to_string(repo.path().join("a.txt"))?),
        "aaa\n",
        "a.txt must be restored to original"
    );
    assert_eq!(
        normalize_newlines(fs::read_to_string(repo.path().join("b.txt"))?),
        "bbb\n",
        "b.txt must be restored to original"
    );
    assert_eq!(
        normalize_newlines(fs::read_to_string(repo.path().join("c.txt"))?),
        "ccc\n",
        "c.txt must be restored to original"
    );
    assert_eq!(
        normalize_newlines(fs::read_to_string(repo.path().join("d.txt"))?),
        "ddd\n",
        "d.txt must be restored to original"
    );
    assert_eq!(
        normalize_newlines(fs::read_to_string(repo.path().join("e.txt"))?),
        "eee\n",
        "e.txt must be restored to original"
    );

    // Uncommitted change must also be restored by git stash pop
    assert_eq!(
        normalize_newlines(fs::read_to_string(repo.path().join("f.txt"))?),
        "working-change\n",
        "uncommitted change must be restored by stash pop"
    );

    Ok(())
}
