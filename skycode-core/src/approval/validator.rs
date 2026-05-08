use std::time::{SystemTime, UNIX_EPOCH};

use ring::signature::{UnparsedPublicKey, ED25519};
use rusqlite::{params, Connection, Error as SqlError, ErrorCode, OptionalExtension};
use thiserror::Error;

use super::token::{ApprovalToken, TokenError};

/// Clock-skew grace period: tokens are accepted up to this many seconds
/// past their nominal expiry to tolerate NTP drift on a single machine.
/// This does NOT extend tokens for any other purpose.
const CLOCK_SKEW_TOLERANCE_SECS: i64 = 5;

#[derive(Debug, Error)]
pub enum ValidatorError {
    #[error("token expired")]
    Expired,
    #[error("diff binding mismatch")]
    DiffBindingMismatch,
    #[error("agent id mismatch: expected {expected}, got {actual}")]
    AgentMismatch { expected: String, actual: String },
    #[error("signature verification failed")]
    InvalidSignature,
    #[error("replay attack detected")]
    ReplayDetected,
    #[error("no signing key registered for agent '{agent_id}' — run `scos approve` first")]
    UnregisteredAgent { agent_id: String },
    #[error("database error: {0}")]
    Database(#[from] rusqlite::Error),
    #[error("token error: {0}")]
    Token(#[from] TokenError),
    #[error("invalid system time")]
    InvalidSystemTime,
}

/// Register (or replace) the ed25519 public key for an agent.
/// Called by `scos approve` so the validator can look up the key without
/// accepting it as a caller-supplied argument.
pub fn register_signing_key(
    conn: &Connection,
    agent_id: &str,
    public_key_hex: &str,
    registered_at: i64,
) -> Result<(), ValidatorError> {
    conn.execute(
        "INSERT OR REPLACE INTO signing_keys (agent_id, public_key_hex, registered_at)
         VALUES (?1, ?2, ?3)",
        params![agent_id, public_key_hex, registered_at],
    )?;
    Ok(())
}

/// Validate an ApprovalToken in 13 steps:
/// 1. TTL check (with clock-skew tolerance)
/// 2. Diff-binding check
/// 3. Agent-id check
/// 4. Public-key lookup from trusted DB (signing_keys table)
/// 5. Signature verification
/// 6. Atomic single-use INSERT (replay defence)
pub fn validate_token(
    conn: &Connection,
    token: &ApprovalToken,
    expected_diff_id: &str,
    expected_agent_id: &str,
    task_id: &str,
) -> Result<(), ValidatorError> {
    // Step 1 — TTL (CHECK 3 fix: CLOCK_SKEW_TOLERANCE_SECS grace period)
    let now = now_unix()?;
    if token.expires_at + CLOCK_SKEW_TOLERANCE_SECS <= now {
        return Err(ValidatorError::Expired);
    }

    // Step 2 — diff binding
    if token.diff_id != expected_diff_id {
        return Err(ValidatorError::DiffBindingMismatch);
    }

    // Step 3 — agent identity
    if token.agent_id != expected_agent_id {
        return Err(ValidatorError::AgentMismatch {
            expected: expected_agent_id.to_string(),
            actual: token.agent_id.clone(),
        });
    }

    // Step 4 — look up the trusted public key from the DB (CHECK 2 fix)
    let key_hex: Option<String> = conn
        .prepare("SELECT public_key_hex FROM signing_keys WHERE agent_id = ?1")?
        .query_row(params![expected_agent_id], |r| r.get(0))
        .optional()?;

    let key_hex = key_hex.ok_or_else(|| ValidatorError::UnregisteredAgent {
        agent_id: expected_agent_id.to_string(),
    })?;

    let public_key = decode_hex(&key_hex).ok_or(ValidatorError::InvalidSignature)?;

    // Step 5 — signature verification
    let payload = token.canonical_payload()?;
    let signature = token.signature_bytes()?;
    let verifier = UnparsedPublicKey::new(&ED25519, &public_key);
    verifier
        .verify(&payload, &signature)
        .map_err(|_| ValidatorError::InvalidSignature)?;

    // Step 6 — atomic single-use INSERT (replay defence)
    let mut stmt = conn.prepare(
        "INSERT INTO approval_tokens_used (tid, diff_id, task_id, used_at)
         VALUES (?1, ?2, ?3, ?4)",
    )?;

    let insert_result =
        stmt.execute(params![token.id.to_string(), token.diff_id, task_id, now,]);

    match insert_result {
        Ok(_) => Ok(()),
        Err(err) if is_constraint_violation(&err) => Err(ValidatorError::ReplayDetected),
        Err(err) => Err(ValidatorError::Database(err)),
    }
}

fn now_unix() -> Result<i64, ValidatorError> {
    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|_| ValidatorError::InvalidSystemTime)?
        .as_secs();

    i64::try_from(secs).map_err(|_| ValidatorError::InvalidSystemTime)
}

fn is_constraint_violation(err: &SqlError) -> bool {
    match err {
        SqlError::SqliteFailure(inner, _) => inner.code == ErrorCode::ConstraintViolation,
        _ => false,
    }
}

/// Decode a lowercase hex string to bytes. Returns None on malformed input.
fn decode_hex(s: &str) -> Option<Vec<u8>> {
    if s.len() % 2 != 0 {
        return None;
    }
    (0..s.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&s[i..i + 2], 16).ok())
        .collect()
}
