use std::time::{SystemTime, UNIX_EPOCH};

use ring::signature::{Ed25519KeyPair, KeyPair};
use serde::{Deserialize, Serialize};
use thiserror::Error;
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApprovalToken {
    pub id: Uuid,
    pub project_id: String,
    pub diff_id: String,
    pub agent_id: String,
    pub key_id: String,
    pub created_at: i64,
    pub expires_at: i64,
    pub signature: String,
    pub nonce: String,
}

#[derive(Debug, Serialize)]
struct ApprovalTokenPayload<'a> {
    project_id: &'a str,
    id: &'a Uuid,
    diff_id: &'a str,
    agent_id: &'a str,
    key_id: &'a str,
    created_at: i64,
    expires_at: i64,
    nonce: &'a str,
}

#[derive(Debug, Error)]
pub enum TokenError {
    #[error("failed to serialize token payload: {0}")]
    Serialize(#[from] serde_json::Error),
    #[error("invalid system time")]
    InvalidSystemTime,
    #[error("invalid signature encoding")]
    InvalidSignatureEncoding,
}

impl ApprovalToken {
    pub fn create_signed(
        project_id: impl Into<String>,
        diff_id: impl Into<String>,
        agent_id: impl Into<String>,
        key_id: impl Into<String>,
        nonce: impl Into<String>,
        key_pair: &Ed25519KeyPair,
    ) -> Result<Self, TokenError> {
        let created_at = now_unix()?;
        let expires_at = created_at + 300;

        let id = Uuid::new_v4();
        let project_id = project_id.into();
        let diff_id = diff_id.into();
        let agent_id = agent_id.into();
        let key_id = key_id.into();
        let nonce = nonce.into();

        let payload = ApprovalTokenPayload {
            project_id: &project_id,
            id: &id,
            diff_id: &diff_id,
            agent_id: &agent_id,
            key_id: &key_id,
            created_at,
            expires_at,
            nonce: &nonce,
        };

        let canonical_payload = serde_json::to_vec(&payload)?;
        let signature = key_pair.sign(&canonical_payload);
        let signature_hex = to_hex(signature.as_ref());

        Ok(Self {
            id,
            project_id,
            diff_id,
            agent_id,
            key_id,
            created_at,
            expires_at,
            signature: signature_hex,
            nonce,
        })
    }

    pub fn canonical_payload(&self) -> Result<Vec<u8>, TokenError> {
        let payload = ApprovalTokenPayload {
            project_id: &self.project_id,
            id: &self.id,
            diff_id: &self.diff_id,
            agent_id: &self.agent_id,
            key_id: &self.key_id,
            created_at: self.created_at,
            expires_at: self.expires_at,
            nonce: &self.nonce,
        };

        Ok(serde_json::to_vec(&payload)?)
    }

    pub fn signature_bytes(&self) -> Result<Vec<u8>, TokenError> {
        from_hex(&self.signature)
    }
}

fn now_unix() -> Result<i64, TokenError> {
    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|_| TokenError::InvalidSystemTime)?
        .as_secs();
    i64::try_from(secs).map_err(|_| TokenError::InvalidSystemTime)
}

fn to_hex(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len() * 2);
    for b in bytes {
        out.push(hex_char((b >> 4) & 0x0f));
        out.push(hex_char(b & 0x0f));
    }
    out
}

fn hex_char(value: u8) -> char {
    match value {
        0..=9 => (b'0' + value) as char,
        _ => (b'a' + (value - 10)) as char,
    }
}

fn from_hex(input: &str) -> Result<Vec<u8>, TokenError> {
    if !input.len().is_multiple_of(2) {
        return Err(TokenError::InvalidSignatureEncoding);
    }

    let mut bytes = Vec::with_capacity(input.len() / 2);
    let chars: Vec<char> = input.chars().collect();

    let mut idx = 0;
    while idx < chars.len() {
        let hi = from_hex_char(chars[idx])?;
        let lo = from_hex_char(chars[idx + 1])?;
        bytes.push((hi << 4) | lo);
        idx += 2;
    }

    Ok(bytes)
}

fn from_hex_char(ch: char) -> Result<u8, TokenError> {
    match ch {
        '0'..='9' => Ok((ch as u8) - b'0'),
        'a'..='f' => Ok((ch as u8) - b'a' + 10),
        'A'..='F' => Ok((ch as u8) - b'A' + 10),
        _ => Err(TokenError::InvalidSignatureEncoding),
    }
}

pub fn public_key_bytes(key_pair: &Ed25519KeyPair) -> Vec<u8> {
    key_pair.public_key().as_ref().to_vec()
}
