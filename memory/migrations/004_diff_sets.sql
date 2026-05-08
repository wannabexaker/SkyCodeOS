-- Phase 6 Pillar 2: UX grouping for multi-file diff proposals.
-- Approval tokens remain bound to individual diff_id values.
-- Note: foreign_keys must be enabled at connection-open time, not here.

CREATE TABLE IF NOT EXISTS diff_sets (
  set_id     TEXT PRIMARY KEY,
  task_id    TEXT NOT NULL,
  agent_id   TEXT NOT NULL,
  project_id TEXT NOT NULL,
  created_at INTEGER NOT NULL
) STRICT;

CREATE TABLE IF NOT EXISTS diff_set_members (
  set_id  TEXT NOT NULL REFERENCES diff_sets(set_id) DEFERRABLE INITIALLY DEFERRED,
  diff_id TEXT NOT NULL,
  ord     INTEGER NOT NULL,
  PRIMARY KEY(set_id, diff_id),
  UNIQUE(set_id, ord)
) STRICT;

CREATE TRIGGER IF NOT EXISTS diff_set_members_no_insert_after_set
BEFORE INSERT ON diff_set_members
WHEN EXISTS (SELECT 1 FROM diff_sets WHERE set_id = NEW.set_id)
BEGIN
  SELECT RAISE(ABORT, 'append-only');
END;

CREATE TRIGGER IF NOT EXISTS diff_set_members_no_update
BEFORE UPDATE ON diff_set_members
BEGIN
  SELECT RAISE(ABORT, 'append-only');
END;

CREATE TRIGGER IF NOT EXISTS diff_set_members_no_delete
BEFORE DELETE ON diff_set_members
BEGIN
  SELECT RAISE(ABORT, 'append-only');
END;
