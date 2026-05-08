use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use clap::Args;
use ring::rand::SystemRandom;
use ring::signature::{Ed25519KeyPair, KeyPair};
use rusqlite::{params, Connection};

use skycode_orchestrator::approval::token::ApprovalToken;
use skycode_orchestrator::approval::validator::register_signing_key;
use skycode_orchestrator::db::migrations::run_migrations;

#[derive(Debug, Args)]
pub struct ApproveArgs {
    /// The UUID of the diff proposal to approve.
    pub diff_id: String,
}

pub fn run(args: &ApproveArgs) -> Result<(), Box<dyn std::error::Error>> {
    let token = approve_diff(&args.diff_id)?;

    println!("Approval token created:");
    println!("  diff_id: {}", token.diff_id);
    println!("  token_id: {}", token.id);
    println!("  expires_at: {}", token.expires_at);

    Ok(())
}

pub fn approve_diff(diff_id: &str) -> Result<ApprovalToken, Box<dyn std::error::Error>> {
    let db_path = std::env::current_dir()?.join("skycode.db");
    let conn = Connection::open(db_path)?;

    let migrations_dir = std::env::current_dir()?.join("memory").join("migrations");
    if migrations_dir.exists() {
        let _ = run_migrations(&conn, &migrations_dir)?;
    }

    ensure_diff_exists(&conn, diff_id)?;

    let key_pair = load_or_create_signing_key()?;
    let token = ApprovalToken::create_signed(
        diff_id.to_string(),
        "coder-primary",
        format!("nonce-{}", now_unix()?),
        &key_pair,
    )?;

    // Bind the public key to the agent identity in the DB so that
    // validate_token can look it up without accepting it as a caller arg (CHECK 2).
    let pub_key_hex: String = key_pair
        .public_key()
        .as_ref()
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect();
    register_signing_key(&conn, "coder-primary", &pub_key_hex, now_unix()?)?;

    persist_token(diff_id, &token)?;

    Ok(token)
}

fn ensure_diff_exists(conn: &Connection, diff_id: &str) -> Result<(), Box<dyn std::error::Error>> {
    let mut stmt = conn.prepare("SELECT 1 FROM diff_proposals WHERE id = ?1 LIMIT 1")?;
    let exists = stmt.exists(params![diff_id])?;
    if !exists {
        return Err(format!("diff_proposals row not found for id: {diff_id}").into());
    }
    Ok(())
}

pub fn load_or_create_signing_key() -> Result<Ed25519KeyPair, Box<dyn std::error::Error>> {
    let key_dir = std::env::current_dir()?.join(".skycode").join("keys");
    fs::create_dir_all(&key_dir)?;
    let key_path = key_dir.join("approval_ed25519.pk8");

    let bytes = if key_path.exists() {
        fs::read(&key_path)?
    } else {
        let rng = SystemRandom::new();
        let pkcs8 = Ed25519KeyPair::generate_pkcs8(&rng)
            .map_err(|_| "failed to generate Ed25519 key pair")?;
        fs::write(&key_path, pkcs8.as_ref())?;
        pkcs8.as_ref().to_vec()
    };

    let key_pair =
        Ed25519KeyPair::from_pkcs8(&bytes).map_err(|_| "failed to parse Ed25519 signing key")?;

    Ok(key_pair)
}

fn persist_token(diff_id: &str, token: &ApprovalToken) -> Result<(), Box<dyn std::error::Error>> {
    let token_dir = std::env::current_dir()?.join(".skycode").join("tokens");
    fs::create_dir_all(&token_dir)?;

    let path = token_path_for(diff_id, &token_dir);
    let payload = serde_json::to_string_pretty(token)?;
    fs::write(path, payload)?;
    Ok(())
}

pub fn load_token(diff_id: &str) -> Result<ApprovalToken, Box<dyn std::error::Error>> {
    let token_dir = std::env::current_dir()?.join(".skycode").join("tokens");
    let path = token_path_for(diff_id, &token_dir);
    let text = fs::read_to_string(path)?;
    Ok(serde_json::from_str(&text)?)
}

pub fn remove_token(diff_id: &str) -> Result<(), Box<dyn std::error::Error>> {
    let token_dir = std::env::current_dir()?.join(".skycode").join("tokens");
    let path = token_path_for(diff_id, &token_dir);
    if path.exists() {
        fs::remove_file(path)?;
    }
    Ok(())
}

fn token_path_for(diff_id: &str, dir: &Path) -> PathBuf {
    dir.join(format!("{}.json", diff_id))
}

fn now_unix() -> Result<i64, Box<dyn std::error::Error>> {
    let secs = SystemTime::now().duration_since(UNIX_EPOCH)?.as_secs();
    Ok(i64::try_from(secs)?)
}
