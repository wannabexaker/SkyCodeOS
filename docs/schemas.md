# Canonical Schemas & Data Models

## SQLite V1 Database

### Schema Configuration

```sql
PRAGMA journal_mode = WAL;
PRAGMA foreign_keys = ON;
```

**Critical requirement:** WAL mode for concurrent read + append operations; foreign keys enforced.

### Memory Tables

```sql
CREATE TABLE memories (
  id          TEXT PRIMARY KEY,
  project_id  TEXT NOT NULL,
  agent_id    TEXT NOT NULL,
  scope       TEXT NOT NULL CHECK (scope IN ('project','agent','session','decision')),
  content     TEXT NOT NULL,
  tags        TEXT,
  importance  REAL NOT NULL DEFAULT 0.5 CHECK (importance >= 0.0 AND importance <= 1.0),
  created_at  INTEGER NOT NULL,
  updated_at  INTEGER NOT NULL,
  last_access INTEGER
) STRICT;
CREATE INDEX idx_memories_proj_agent ON memories(project_id, agent_id);
CREATE INDEX idx_memories_scope ON memories(scope);

CREATE VIRTUAL TABLE memories_fts USING fts5(
  content, tags,
  content='memories', content_rowid='rowid',
  tokenize='porter unicode61'
);
CREATE TRIGGER memories_ai AFTER INSERT ON memories BEGIN
  INSERT INTO memories_fts(rowid, content, tags) VALUES (new.rowid, new.content, new.tags);
END;
CREATE TRIGGER memories_au AFTER UPDATE ON memories BEGIN
  INSERT INTO memories_fts(memories_fts, rowid, content, tags)
  VALUES('delete', old.rowid, old.content, old.tags);
  INSERT INTO memories_fts(rowid, content, tags) VALUES (new.rowid, new.content, new.tags);
END;
```

### Event Sourcing Tables

```sql
-- Audit log (append-only, event-sourced)
CREATE TABLE tool_events (
  id                  TEXT PRIMARY KEY,
  task_id             TEXT NOT NULL,
  agent_id            TEXT NOT NULL,
  event_type          TEXT NOT NULL CHECK (event_type IN (
    'tool_requested','diff_proposed','diff_approved','diff_rejected',
    'diff_applied','diff_apply_failed','rollback_requested','rollback_applied',
    'rollback_failed','policy_denied','secret_redacted','model_invoked',
    'model_failed','memory_written','decision_written',
    'context_budget_enforced','trust_check_failed','tuning_run_started',
    'tuning_run_completed','migration_destructive_applied'
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
CREATE INDEX idx_tool_events_task ON tool_events(task_id, created_at);
CREATE INDEX idx_tool_events_type ON tool_events(event_type);
CREATE INDEX idx_tool_events_diff ON tool_events(diff_id);
CREATE TRIGGER tool_events_no_update BEFORE UPDATE ON tool_events BEGIN
  SELECT RAISE(ABORT, 'tool_events is append-only');
END;
CREATE TRIGGER tool_events_no_delete BEFORE DELETE ON tool_events BEGIN
  SELECT RAISE(ABORT, 'tool_events is append-only');
END;

-- Immutable diffs
CREATE TABLE diff_proposals (
  id                     TEXT PRIMARY KEY,
  task_id                TEXT NOT NULL,
  agent_id               TEXT NOT NULL,
  project_id             TEXT NOT NULL,
  patch_unified          TEXT NOT NULL,
  base_git_ref           TEXT NOT NULL,
  base_blob_hashes_json  TEXT NOT NULL,
  affected_files_json    TEXT NOT NULL,
  created_at             INTEGER NOT NULL,
  expires_at             INTEGER
) STRICT;
CREATE INDEX idx_diffs_task ON diff_proposals(task_id);
CREATE INDEX idx_diffs_project ON diff_proposals(project_id);
CREATE TRIGGER diff_proposals_no_update BEFORE UPDATE ON diff_proposals BEGIN
  SELECT RAISE(ABORT, 'diff_proposals is immutable');
END;
CREATE TRIGGER diff_proposals_no_delete BEFORE DELETE ON diff_proposals BEGIN
  SELECT RAISE(ABORT, 'diff_proposals is immutable');
END;

-- Approval token replay defense
CREATE TABLE approval_tokens_used (
  tid       TEXT PRIMARY KEY,
  diff_id   TEXT NOT NULL,
  task_id   TEXT NOT NULL,
  used_at   INTEGER NOT NULL
) STRICT;
CREATE TRIGGER approval_tokens_used_no_update BEFORE UPDATE ON approval_tokens_used BEGIN
  SELECT RAISE(ABORT, 'approval_tokens_used is append-only');
END;
CREATE TRIGGER approval_tokens_used_no_delete BEFORE DELETE ON approval_tokens_used BEGIN
  SELECT RAISE(ABORT, 'approval_tokens_used is append-only');
END;

-- Immutable applied-change record
CREATE TABLE applied_changes (
  id                    TEXT PRIMARY KEY,
  task_id               TEXT NOT NULL,
  diff_id               TEXT NOT NULL,
  project_id            TEXT NOT NULL,
  pre_apply_git_ref     TEXT NOT NULL,
  post_apply_git_ref    TEXT,
  apply_branch          TEXT NOT NULL,
  affected_files_json   TEXT NOT NULL,
  applied_at            INTEGER NOT NULL
) STRICT;
CREATE INDEX idx_applied_changes_task ON applied_changes(task_id);
CREATE TRIGGER applied_changes_no_update BEFORE UPDATE ON applied_changes BEGIN
  SELECT RAISE(ABORT, 'applied_changes is immutable');
END;
CREATE TRIGGER applied_changes_no_delete BEFORE DELETE ON applied_changes BEGIN
  SELECT RAISE(ABORT, 'applied_changes is immutable');
END;
```

### Graph Tables

```sql
CREATE TABLE graph_nodes (
  id            TEXT PRIMARY KEY,
  project_id    TEXT NOT NULL,
  kind          TEXT NOT NULL CHECK (kind IN ('file','folder','symbol','import','export')),
  name          TEXT NOT NULL,
  path          TEXT,
  language      TEXT,
  span_json     TEXT,
  metadata_json TEXT,
  updated_at    INTEGER NOT NULL
) STRICT;
CREATE INDEX idx_graph_nodes_proj_kind ON graph_nodes(project_id, kind);
CREATE INDEX idx_graph_nodes_path ON graph_nodes(path);

CREATE TABLE graph_edges (
  id            TEXT PRIMARY KEY,
  project_id    TEXT NOT NULL,
  from_id       TEXT NOT NULL REFERENCES graph_nodes(id) ON DELETE CASCADE,
  to_id         TEXT NOT NULL REFERENCES graph_nodes(id) ON DELETE CASCADE,
  kind          TEXT NOT NULL CHECK (kind IN ('contains','imports','exports','depends_on','tested_by','calls')),
  metadata_json TEXT
) STRICT;
CREATE INDEX idx_edges_from ON graph_edges(from_id);
CREATE INDEX idx_edges_to ON graph_edges(to_id);
CREATE INDEX idx_edges_kind ON graph_edges(kind);
```

### Tuning Lab (Patch 21)

```sql
CREATE TABLE tuning_runs (
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
CREATE INDEX idx_tuning_runs_model_profile ON tuning_runs(model_name, profile_name);
CREATE INDEX idx_tuning_runs_created ON tuning_runs(created_at);
```

### Metadata Tables

```sql
-- Decisions and reasoning
CREATE TABLE decisions (
  id            TEXT PRIMARY KEY,
  project_id    TEXT NOT NULL,
  agent_id      TEXT NOT NULL,
  task_id       TEXT NOT NULL,
  summary       TEXT NOT NULL,
  rationale     TEXT,
  context_refs  TEXT,
  outcome       TEXT NOT NULL CHECK (outcome IN ('approved','rejected','rolled_back')),
  created_at    INTEGER NOT NULL
) STRICT;
CREATE INDEX idx_decisions_task ON decisions(task_id);
CREATE INDEX idx_decisions_proj_agent ON decisions(project_id, agent_id);

-- Agent session state
CREATE TABLE agent_state (
  agent_id     TEXT NOT NULL,
  project_id   TEXT NOT NULL,
  state_json   TEXT NOT NULL,
  session_id   TEXT,
  updated_at   INTEGER NOT NULL,
  PRIMARY KEY (agent_id, project_id)
) STRICT;

-- Schema migration ledger
CREATE TABLE _skycode_migrations (
  version    INTEGER PRIMARY KEY,
  applied_at INTEGER NOT NULL,
  sha256     TEXT NOT NULL
) STRICT;

-- Reserved for V2 multi-agent
CREATE TABLE relationships (
  agent_id    TEXT NOT NULL,
  target_id   TEXT NOT NULL,
  note        TEXT,
  created_at  INTEGER NOT NULL,
  PRIMARY KEY (agent_id, target_id)
) STRICT;
```

---

## Retrieval Ranking (V1)

No embeddings. Hybrid BM25 + recency + importance scoring:

```
score = bm25(memories_fts) * recency_decay(now - last_access) * importance * scope_match(query.scope)

recency_decay(dt) = exp(-dt / TAU)
  where TAU = 14 days (configurable per scope)

scope_match:
  1.0 if query.scope == memory.scope
  0.5 if scopes are compatible (e.g., session ↔ agent)
  0.0 if scopes don't match (e.g., project ↔ session)
```

---

## Impact Query (Graph Dependencies)

```sql
WITH RECURSIVE deps(id, depth) AS (
  SELECT from_id, 1 FROM graph_edges
   WHERE to_id = :target_id AND kind IN ('imports','calls','depends_on')
  UNION
  SELECT e.from_id, d.depth + 1
    FROM graph_edges e JOIN deps d ON e.to_id = d.id
   WHERE d.depth < :max_depth
)
SELECT n.* FROM graph_nodes n WHERE n.id IN (SELECT id FROM deps);
```

Returns all nodes that depend on a given symbol, file, or module. Used for:
- Estimating refactoring scope
- Understanding blast radius of changes
- Building context for suggestions

---

## Decisions Table

Structured logging of approvals, rejections, and rollbacks:

```sql
INSERT INTO decisions(id, project_id, agent_id, task_id, summary, rationale, outcome, created_at)
VALUES(?, ?, ?, ?, ?, ?, ?, strftime('%s','now'));
```

Agent recalls decisions via memory search on task description + decision summary.

---

## Approval Token Usage

Single-use enforcement via primary key constraint:

```sql
-- Token validation step 9 from protocol.md
INSERT INTO approval_tokens_used(tid, diff_id, task_id, used_at)
VALUES(:token_id, :diff_id, :task_id, strftime('%s', 'now'));
-- PRIMARY KEY violation → replay detected → reject
```

Audit query to verify token usage:

```sql
SELECT t.* FROM approval_tokens_used t
 WHERE t.diff_id = :diff_id
 ORDER BY t.used_at DESC
 LIMIT 1;
```

