CREATE TABLE IF NOT EXISTS submitted_tasks (
    id TEXT PRIMARY KEY,
    agent_id TEXT NOT NULL,
    goal TEXT NOT NULL,
    mode TEXT NOT NULL DEFAULT 'diff',
    status TEXT NOT NULL DEFAULT 'accepted',
    quest_id TEXT,
    guild_id TEXT,
    external_ref TEXT,
    created_at INTEGER NOT NULL
);

CREATE TABLE IF NOT EXISTS tool_events_phase9 (
  id                  TEXT PRIMARY KEY,
  task_id             TEXT NOT NULL,
  agent_id            TEXT NOT NULL,
  event_type          TEXT NOT NULL CHECK (event_type IN (
    'tool_requested','diff_proposed','diff_approved','diff_rejected',
    'diff_applied','diff_apply_failed','rollback_requested','rollback_applied',
    'rollback_failed','policy_denied','secret_redacted','model_invoked',
    'model_failed','memory_written','decision_written',
    'context_budget_enforced','trust_check_failed','tuning_run_started',
    'tuning_run_completed','migration_destructive_applied',
    'test_verify_passed','apply_unverified',
    'agent.turn.started','agent.turn.completed','model.invoked',
    'tool.requested','tool.completed','diff.proposed','diff.approved',
    'diff.applied','verify.passed','apply.unverified','memory.retrieved',
    'security.blocked'
  )),
  tool_name           TEXT,
  inputs_hash         TEXT,
  inputs_json         TEXT,
  output_hash         TEXT,
  output_json         TEXT,
  approval_token_id   TEXT,
  diff_id             TEXT,
  profile_name        TEXT,
  created_at          INTEGER NOT NULL
) STRICT;

INSERT OR IGNORE INTO tool_events_phase9 (
  id, task_id, agent_id, event_type, tool_name,
  inputs_hash, inputs_json, output_hash, output_json,
  approval_token_id, diff_id, profile_name, created_at
)
SELECT
  id, task_id, agent_id, event_type, tool_name,
  inputs_hash, inputs_json, output_hash, output_json,
  approval_token_id, diff_id, profile_name, created_at
FROM tool_events;

DROP TRIGGER IF EXISTS tool_events_no_update;
DROP TRIGGER IF EXISTS tool_events_no_delete;
DROP TABLE IF EXISTS tool_events;
ALTER TABLE tool_events_phase9 RENAME TO tool_events;

CREATE INDEX IF NOT EXISTS idx_tool_events_task ON tool_events(task_id, created_at);
CREATE INDEX IF NOT EXISTS idx_tool_events_type ON tool_events(event_type);
CREATE INDEX IF NOT EXISTS idx_tool_events_diff ON tool_events(diff_id);

CREATE TRIGGER IF NOT EXISTS tool_events_no_update BEFORE UPDATE ON tool_events BEGIN
  SELECT RAISE(ABORT, 'tool_events is append-only');
END;

CREATE TRIGGER IF NOT EXISTS tool_events_no_delete BEFORE DELETE ON tool_events BEGIN
  SELECT RAISE(ABORT, 'tool_events is append-only');
END;
