use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

use ring::rand::SystemRandom;
use ring::signature::Ed25519KeyPair;
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

    let token_d1 = ApprovalToken::create_signed("d1", agent_id, "nonce-d1", &key_pair)?;
    let token_d2 = ApprovalToken::create_signed("d2", agent_id, "nonce-d2", &key_pair)?;
    let token_d3 = ApprovalToken::create_signed("d3", agent_id, "nonce-d3", &key_pair)?;

    validate_token(&conn, &token_d1, "d1", agent_id, task_id)?;
    validate_token(&conn, &token_d2, "d2", agent_id, task_id)?;
    validate_token(&conn, &token_d3, "d3", agent_id, task_id)?;

    let mismatch = validate_token(&conn, &token_d1, "d2", agent_id, task_id)
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
