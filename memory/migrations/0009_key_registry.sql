ALTER TABLE signing_keys ADD COLUMN key_id TEXT;

UPDATE signing_keys
SET key_id = agent_id
WHERE key_id IS NULL;

CREATE UNIQUE INDEX IF NOT EXISTS idx_signing_keys_key_id
ON signing_keys(key_id);
