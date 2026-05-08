-- Migration 003: signing_keys
-- Binds the ed25519 public key to an agent identity in the DB.
-- The approval validator looks up the key here rather than accepting it
-- as a caller-supplied argument (CHECK 2 security fix).
CREATE TABLE IF NOT EXISTS signing_keys (
    agent_id       TEXT PRIMARY KEY,
    public_key_hex TEXT NOT NULL,
    registered_at  INTEGER NOT NULL
) STRICT;
