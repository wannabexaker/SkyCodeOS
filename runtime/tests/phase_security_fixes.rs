use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use ring::rand::SystemRandom;
use ring::signature::Ed25519KeyPair;
use rusqlite::Connection;
use skycode_runtime::approval::token::{public_key_bytes, ApprovalToken};
use skycode_runtime::approval::validator::{
    register_signing_key_with_key_id, validate_token, ValidatorError,
};
use skycode_runtime::db::migrations::run_migrations;

const AGENT_ID: &str = "coder-primary";
const PROJECT_ID: &str = "default";
const DIFF_ID: &str = "diff-security";
const TASK_ID: &str = "task-security";
const KEY_A_ID: &str = "key-a";

#[test]
fn sec_forgery_rejects_wrong_key() -> Result<(), Box<dyn std::error::Error>> {
    let conn = migrated_conn()?;
    let key_a = make_keypair()?;
    let key_b = make_keypair()?;
    register_key(&conn, AGENT_ID, KEY_A_ID, &key_a)?;

    let mut tampered =
        ApprovalToken::create_signed(PROJECT_ID, DIFF_ID, AGENT_ID, KEY_A_ID, "nonce-a", &key_a)?;
    tampered.nonce = "attacker-changed-payload".to_string();

    let err = validate_token(&conn, &tampered, PROJECT_ID, DIFF_ID, AGENT_ID, TASK_ID)
        .expect_err("tampered token payload must fail signature verification");
    assert!(matches!(err, ValidatorError::InvalidSignature));

    let signed_by_unregistered_key =
        ApprovalToken::create_signed(PROJECT_ID, DIFF_ID, AGENT_ID, KEY_A_ID, "nonce-b", &key_b)?;

    let err = validate_token(
        &conn,
        &signed_by_unregistered_key,
        PROJECT_ID,
        DIFF_ID,
        AGENT_ID,
        TASK_ID,
    )
    .expect_err("token signed by key B but claiming key A must fail");
    assert!(matches!(err, ValidatorError::InvalidSignature));

    Ok(())
}

#[test]
fn sec_forgery_rejects_unregistered_key_id() -> Result<(), Box<dyn std::error::Error>> {
    let conn = migrated_conn()?;
    let key_pair = make_keypair()?;
    let token = ApprovalToken::create_signed(
        PROJECT_ID,
        DIFF_ID,
        AGENT_ID,
        "not-in-registry",
        "nonce-unregistered",
        &key_pair,
    )?;

    let err = validate_token(&conn, &token, PROJECT_ID, DIFF_ID, AGENT_ID, TASK_ID)
        .expect_err("unknown key_id must be rejected");

    assert!(matches!(err, ValidatorError::UnknownKeyId { .. }));
    assert!(
        err.to_string().contains("unknown key_id"),
        "error should mention unknown key_id, got: {err}"
    );

    Ok(())
}

#[test]
fn sec_clock_skew_within_grace() -> Result<(), Box<dyn std::error::Error>> {
    let conn = migrated_conn()?;
    let key_pair = make_keypair()?;
    register_key(&conn, AGENT_ID, KEY_A_ID, &key_pair)?;

    let mut token = ApprovalToken::create_signed(
        PROJECT_ID,
        DIFF_ID,
        AGENT_ID,
        KEY_A_ID,
        "nonce-within-grace",
        &key_pair,
    )?;
    set_expiry_and_resign(&mut token, &key_pair, unix_now()? - 25)?;

    validate_token(&conn, &token, PROJECT_ID, DIFF_ID, AGENT_ID, TASK_ID)?;

    Ok(())
}

#[test]
fn sec_clock_skew_beyond_grace() -> Result<(), Box<dyn std::error::Error>> {
    let conn = migrated_conn()?;
    let key_pair = make_keypair()?;
    register_key(&conn, AGENT_ID, KEY_A_ID, &key_pair)?;

    let mut token = ApprovalToken::create_signed(
        PROJECT_ID,
        DIFF_ID,
        AGENT_ID,
        KEY_A_ID,
        "nonce-beyond-grace",
        &key_pair,
    )?;
    set_expiry_and_resign(&mut token, &key_pair, unix_now()? - 35)?;

    let err = validate_token(&conn, &token, PROJECT_ID, DIFF_ID, AGENT_ID, TASK_ID)
        .expect_err("token expired beyond grace must reject");

    assert!(matches!(err, ValidatorError::Expired));

    Ok(())
}

fn migrated_conn() -> Result<Connection, Box<dyn std::error::Error>> {
    let conn = Connection::open_in_memory()?;
    run_migrations(&conn, &migrations_dir())?;
    Ok(conn)
}

fn migrations_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("memory")
        .join("migrations")
}

fn make_keypair() -> Result<Ed25519KeyPair, Box<dyn std::error::Error>> {
    let rng = SystemRandom::new();
    let pkcs8 =
        Ed25519KeyPair::generate_pkcs8(&rng).map_err(|_| "failed to generate Ed25519 key")?;
    Ed25519KeyPair::from_pkcs8(pkcs8.as_ref()).map_err(|_| "failed to parse Ed25519 key".into())
}

fn register_key(
    conn: &Connection,
    agent_id: &str,
    key_id: &str,
    key_pair: &Ed25519KeyPair,
) -> Result<(), Box<dyn std::error::Error>> {
    let public_key_hex = public_key_bytes(key_pair)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect::<String>();

    register_signing_key_with_key_id(conn, agent_id, key_id, &public_key_hex, unix_now()?)?;
    Ok(())
}

fn set_expiry_and_resign(
    token: &mut ApprovalToken,
    key_pair: &Ed25519KeyPair,
    expires_at: i64,
) -> Result<(), Box<dyn std::error::Error>> {
    token.expires_at = expires_at;
    token.created_at = expires_at - 300;
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

fn unix_now() -> Result<i64, Box<dyn std::error::Error>> {
    let secs = SystemTime::now().duration_since(UNIX_EPOCH)?.as_secs();
    Ok(i64::try_from(secs)?)
}
