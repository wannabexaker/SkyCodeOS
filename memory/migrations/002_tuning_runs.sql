-- SkyCodeOS V1 - Tuning Lab schema.
-- Idempotent upgrade for databases created before tuning_runs existed.

CREATE TABLE IF NOT EXISTS tuning_runs (
  id                 TEXT PRIMARY KEY,
  project_id         TEXT,
  model_name         TEXT NOT NULL,
  profile_name       TEXT NOT NULL,
  task_class         TEXT,
  prompt_hash        TEXT NOT NULL,
  settings_json      TEXT NOT NULL,
  result_summary     TEXT,
  first_token_ms     INTEGER,
  decode_tok_s       REAL,
  prompt_eval_tok_s  REAL,
  peak_vram_mb       INTEGER,
  peak_ram_mb        INTEGER,
  ctx_requested      INTEGER,
  ctx_achieved       INTEGER,
  output_tokens      INTEGER,
  error_code         TEXT,
  created_at         INTEGER NOT NULL
) STRICT;

CREATE INDEX IF NOT EXISTS idx_tuning_runs_model_profile ON tuning_runs(model_name, profile_name);
CREATE INDEX IF NOT EXISTS idx_tuning_runs_created ON tuning_runs(created_at);
