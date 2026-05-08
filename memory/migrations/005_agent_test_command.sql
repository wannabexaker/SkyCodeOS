-- Phase 6 Pillar 3: per-agent test verification command settings.
-- This preserves the Phase 4 agent_state payload columns and adds the
-- verification fields used by the orchestrator policy layer.

CREATE TABLE IF NOT EXISTS agent_state (
  agent_id            TEXT NOT NULL,
  project_id          TEXT NOT NULL DEFAULT 'default',
  state_json          TEXT NOT NULL DEFAULT '{}',
  session_id          TEXT,
  updated_at          INTEGER NOT NULL DEFAULT (unixepoch()),
  PRIMARY KEY (agent_id, project_id)
) STRICT;

ALTER TABLE agent_state ADD COLUMN test_command TEXT;
ALTER TABLE agent_state ADD COLUMN verify_timeout_secs INTEGER NOT NULL DEFAULT 60;
