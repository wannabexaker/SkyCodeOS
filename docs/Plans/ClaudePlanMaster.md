# Skycode V1 — Canonical Implementation Plan (Master)

This is the master canonical plan for Skycode V1. It supersedes `docs/PLAN.md` (first canonical pass) and every plan in `docs/Plans/*.md` (ChatgptPlan, ChatgptPlanV2, GithubCopilotPlan, GeminiPlan, GeminiPlanV2, ClaudePlan, ClaudePlanV2, DeepSeekPlan, DeepSeekPlanV2). All historical plans are now read-only input.

Precedence:

```
docs/*.md  >  docs/Plans/ClaudePlanMaster.md  >  docs/PLAN.md  >  historical *Plan*.md  >  source comments
```

If `/docs/*.md` and this plan disagree, `/docs` wins. Where this plan refines a `/docs` rule with operational detail (not contradiction), this plan stands.

---

## 1. Executive Summary

Skycode V1 is one persistent local coder agent (`coder-primary`) behind a strict layered runtime: `Models → Inference Runtime → SkyCore Protocol → Orchestrator → Agent Runtime / Memory + Graph / Tools → CLI`. The orchestrator owns model invocation, tool execution, context retrieval, approval gates, policy enforcement, event logging, and memory writes. The agent runtime holds identity, intent, and state — and *no* filesystem, model, network, process, or tool handle. Diffs are produced as proposals, persisted immutably, signed with single-use ed25519 tokens (TTL=300s), and applied onto isolated branches without moving user HEAD. Memory is SQLite + FTS5; the project graph is built with tree-sitter. Inference is local llama.cpp GGUF; remote adapters exist as code paths but ship `enabled: false`. CLI is the only V1 interface.

Reviewer agent, manager/architect, Tauri UI, vector embeddings, multi-agent orchestration, remote fallback enabled by default, voice, multimodal, swarm, personality drift, emotional valence, trust scoring, and active relationship memory are post-V1, gate-locked. Not negotiable.

V1 is successful when a developer runs `skycode trust . && skycode doctor && skycode model verify local-coder && skycode scan . && skycode profile use precise && skycode ask "<refactor>"` on a 50k-line repository with the network disabled, gets a graph-derived diff proposal, approves it via a signed single-use token, and the agent recalls the decision three sessions and two restarts later — with zero unapproved writes in the audit log, ≥50% context-token reduction versus naive file-dump, and all architectural dependency tests green.

V1 must run on a worst-case 2018-class workstation: 6GB VRAM (GTX 1060 class), 4-core no-AVX-512 CPU (i3-8100 class), 24GB DDR4, PCIe Gen 3, SATA SSD, offline. Better hardware is upside, not requirement. Phase 5 hardening must include a benchmark on this exact class. Hardware acceptance is tiered (required minimum / target benchmark / acceptable fallback) per §5 and §11.

---

## 2. Non-Negotiables

1. **No silent writes.** File mutation only via `apply_diff(diff_id, approval_token)`. Any other write path is a release blocker.
2. **Layer separation strictly enforced.** Forbidden crossings have crate-level controls, not just convention.
3. **Local-first.** Every V1 exit gate must pass with the network disabled at the OS level.
4. **No full-repo context dumps.** Context is built from `memory:*` and `graph:*` refs, bounded by an explicit context budget.
5. **Agent never bypasses orchestrator.** Tool execution, model invocation, memory writes, and SkyCore request construction live in the orchestrator. The agent runtime returns structured intent and parses normalized SkyCore responses.
6. **SQLite + FTS5 only for memory.** No vector DB, no embeddings until FTS5 measurably fails a labelled retrieval benchmark.
7. **Single agent before multi-agent.** Exactly one agent (`coder-primary`) exists in V1.
8. **All-Rust runtime past Phase 3.** No Python in the runtime path. Agent definitions are YAML.
9. **Append-only event sourcing.** `tool_events`, `diff_proposals`, `applied_changes`, `approval_tokens_used`, `_skycode_migrations` are append-only at the application layer; triggers block `UPDATE`/`DELETE`.
10. **Signed single-use approval token.** ed25519, UUIDv4 token id, TTL=300s, scope-bound, diff-id-bound, task-id-bound; replay defended by a primary-key insert into `approval_tokens_used`.
11. **Provider format never crosses SkyCore.** Architectural test enforces.
12. **Remote adapter disabled by default.** Optional, isolated, must not affect any V1 gate when off.
13. **Reviewer agent is post-V1.** Not present in any V1 phase under any framing.
14. **Speculative decoding off by default.** Documented rationale in §5.
15. **Model registry is data, not code.** Hot-reloadable YAML; no model name hardcoded in Rust.
16. **Worst-case hardware is the V1 baseline.** §HARDWARE TARGET sign-off uses the §5 Target tier; `local-fallback` covers hosts that cannot reach the target.
17. **Project trust required for writes.** Untrusted projects are read-only-answer mode (Patch 9).
18. **Secrets never enter prompts, memory, FTS5, graph, or logs unredacted** (Patch 7).
19. **Profiles are tuning, not policy.** No profile may enable remote, weaken approval, widen tools, disable secret scanning, or disable audit (Patch 21).

---

## 3. Layer Architecture

```
+-----------------------------------------------------------------+
|                            CLI (V1)                             |
+-----------------------------------------------------------------+
                              | user commands
                              v
+-----------------------------------------------------------------+
|                          Orchestrator                           |
|   policy engine | approval gate | event log writer | router*    |
|   context builder | SkyCore client | tool executor              |
|   memory writer  | decision writer | secret/policy enforcer     |
|   profile resolver | trust enforcer                             |
+-----------------------------------------------------------------+
   |              |              |              |              |
   | identity     | context      | tool calls   | SkyCore       | events
   v              v              v              v              v
+--------+  +-----------+  +---------+  +-------------+  +-----------+
| Agent  |  | Memory +  |  | Tools   |  | Inference   |  | Audit Log |
| Run-   |  | Graph     |  | (read/  |  | Runtime     |  | (tool_    |
| time   |  | (SQLite + |  | diff/   |  | (llama.cpp  |  |  events,  |
| (no    |  |  FTS5 +   |  | apply/  |  |  + adapter*)|  |  diff_    |
| handles)| | tree-     |  | git)    |  |             |  |  props,   |
|        |  | sitter)   |  |         |  |             |  |  approvals|
+--------+  +-----------+  +---------+  +-------------+  |  changes, |
                                                         |  tuning)  |
                                                         +-----------+
* router and remote adapter both restricted: router additive in Phase 5;
  remote adapter exists as code path but is `enabled: false` by default.
```

**Agent Runtime (closed responsibility list):**
- Loads identity from `agents/<id>/core/{soul,heart,mind,doctrine}.yaml`.
- Holds and persists task-scoped working state via `agent_state`.
- Builds `AgentIntent { goal, constraints, requested_tools, output_contract, context_hints }`.
- Renders prompt fragments from identity + intent + context **handed in by orchestrator**.
- Parses normalized SkyCore responses into agent-level outputs (`DiffProposal`, `Answer`, `Plan`).

**Agent Runtime forbidden APIs** (lint + crate-deny enforced): `std::fs`, `std::net`, `std::process`, any `skycode-tools` symbol, any `skycode-inference` symbol, any `skycode-memory`/`skycode-graph` write API. `skycode-runtime` depends on `skycode-core` only.

**Allowed boundary crossings (the only permitted edges):**

- `CLI → Orchestrator` — user commands.
- `Orchestrator → Agent Runtime` — identity load, intent build, response parse.
- `Orchestrator → Memory/Graph` — read for context; write decisions/memories/state.
- `Orchestrator → Tools` — tool execution under policy.
- `Orchestrator → SkyCore client → Inference Runtime` — model invocation only.
- `Inference Runtime → Models` — GGUF/local; remote adapter only when explicitly enabled.
- `Tools → filesystem/git` — reads free; writes only via `apply_diff(diff_id, token)`.
- `Graph scanner → filesystem` — read-only; SQLite-only writes.

**Forbidden crossings (each paired with control):**

- `Agent Runtime → Inference Runtime` → no `skycode-inference` dep in `skycode-runtime`; arch test.
- `Agent Runtime → Tools` → no `skycode-tools` dep in `skycode-runtime`; arch test.
- `Agent Runtime → filesystem/network/process` → lint bans `std::fs`, `std::net`, `std::process` in `skycode-runtime`.
- `Agent Runtime → Memory/Graph write` → runtime consumes a read-only `ContextProvider` trait; the writer trait lives only in orchestrator.
- `CLI → Inference/Memory/Graph/Tools direct` → cli depends only on orchestrator.
- `Tools public write outside apply_diff` → write fns private; `apply_diff` is the sole public mutator; first statement is token validation.
- `Provider format above SkyCore` → integration test scans every value reaching Orchestrator/Agent Runtime/CLI for raw provider fields.
- `Audit-table mutation` → triggers in §4.4 abort `UPDATE`/`DELETE` on `tool_events`, `diff_proposals`, `applied_changes`, `approval_tokens_used`.
- `Remote adapter on by default` → registry default `enabled: false`; offline CI test asserts no socket open.
- `Untrusted project write` → trust enforcer (Patch 9) blocks writes/terminal/remote in untrusted mode before policy runs.
- `Secret-bearing context to model` → secret redactor (Patch 7) runs immediately before SkyCore request build; unredactable matches abort.
- `Profile widening policy` → profile loader (Patch 21) rejects profiles that touch policy/approval/tools/remote/secrets/audit fields.

---

## 4. Canonical Schemas

### 4.1 SkyCore request

```json
{
  "skycore_version": "0.1",
  "task_id": "uuid-v4",
  "agent_id": "coder-primary",
  "goal": "string",
  "context_refs": [
    "memory:<id>",
    "graph:<kind>:<id>",
    "file:<repo-relative-path>",
    "decision:<id>"
  ],
  "tools_allowed": [
    "read_file", "list_dir", "search_project",
    "git_status", "git_diff", "create_diff"
  ],
  "model_policy": {
    "preferred": "local-coder",
    "fallback": "local-fallback",
    "profile":  "precise"
  },
  "output_contract": "diff_proposal",
  "constraints": {
    "max_output_tokens": 4096,
    "stream": true,
    "stop": []
  }
}
```

`output_contract` ∈ `{ "diff_proposal", "answer", "plan" }`. `model_policy.profile` is required; resolved per Patch 21.

### 4.2 SkyCore response

```json
{
  "skycore_version": "0.1",
  "task_id": "uuid-v4",
  "status": "ok",
  "summary": "string",
  "artifacts": [
    { "kind": "diff", "id": "patch-001", "patch_unified": "..." },
    { "kind": "memory", "id": "mem-..." }
  ],
  "tool_calls_requested": [
    { "tool": "search_project", "inputs": { "query": "AuthService" } }
  ],
  "requires_approval": true,
  "error": null
}
```

`status` ∈ `{ "ok", "error", "needs_approval", "needs_tool" }`. `error` is `null` or `{ "code", "message" }`. No provider field (`choices`, `delta`, raw `usage`, raw `role` strings) is permitted at this layer.

### 4.3 Approval token contract

```
token := base64url( payload || "." || ed25519_sig(payload) )
payload := {
  "tid":  "<uuid-v4>",          // token id, single-use; PK in approval_tokens_used
  "scp":  "apply_diff",         // scope; only this scope grants writes
  "did":  "<diff-id>",          // diff_id binding
  "tsk":  "<task-id>",          // origin task
  "exp":  <unix-seconds>,       // now + 300
  "kid":  "<key-id>"            // signing key id, rotatable
}
```

Validation order (fail fast on first failure):

1. Decode base64url; split payload and signature.
2. Parse payload JSON; verify schema.
3. Resolve signing key by `payload.kid`. Reject on unknown.
4. Verify ed25519 signature over canonical payload bytes. Reject on failure.
5. Reject if `payload.exp <= now()`.
6. Reject if `payload.scp != "apply_diff"`.
7. Reject if `payload.did` ≠ `diff_id` argument.
8. Reject if `payload.tsk` ≠ resolved task for the diff.
9. Atomic `INSERT INTO approval_tokens_used(tid, diff_id, task_id, used_at) VALUES (?, ?, ?, ?)`. PK violation → reject as replay.
10. Load immutable `diff_proposals` row by `payload.did`. Reject if missing, expired, or scoped to a different project than the working dir.
11. Verify `base_blob_hashes_json` against current working tree. Mismatch → `diff_apply_failed` event, exit 4.
12. Apply patch onto isolated branch per §4.12 git isolation.
13. Append `diff_applied` event with `approval_token_id = payload.tid` and `diff_id = payload.did`.

The token never carries diff bytes; the diff is loaded from `diff_proposals`.

### 4.4 SQLite DDL

```sql
PRAGMA journal_mode = WAL;
PRAGMA foreign_keys = ON;

-- Memory ----------------------------------------------------------------
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
CREATE TRIGGER memories_ad AFTER DELETE ON memories BEGIN
  INSERT INTO memories_fts(memories_fts, rowid, content, tags)
  VALUES('delete', old.rowid, old.content, old.tags);
END;
CREATE TRIGGER memories_au AFTER UPDATE ON memories BEGIN
  INSERT INTO memories_fts(memories_fts, rowid, content, tags)
  VALUES('delete', old.rowid, old.content, old.tags);
  INSERT INTO memories_fts(rowid, content, tags) VALUES (new.rowid, new.content, new.tags);
END;

-- Decisions -------------------------------------------------------------
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

-- Agent state -----------------------------------------------------------
CREATE TABLE agent_state (
  agent_id     TEXT NOT NULL,
  project_id   TEXT NOT NULL,
  state_json   TEXT NOT NULL,
  session_id   TEXT,
  updated_at   INTEGER NOT NULL,
  PRIMARY KEY (agent_id, project_id)
) STRICT;

-- Tool events (append-only, event-sourced) ------------------------------
CREATE TABLE tool_events (
  id                  TEXT PRIMARY KEY,                        -- sha256(canonical(payload))
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
  profile_name        TEXT,                                    -- (Patch 21)
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

-- Diff proposals (immutable) -------------------------------------------
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

-- Approval token replay defense ----------------------------------------
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

-- Applied changes (immutable) ------------------------------------------
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

-- Graph -----------------------------------------------------------------
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

-- Tuning lab (Patch 21) -------------------------------------------------
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

-- Migrations ledger -----------------------------------------------------
CREATE TABLE _skycode_migrations (
  version    INTEGER PRIMARY KEY,
  applied_at INTEGER NOT NULL,
  sha256     TEXT NOT NULL
) STRICT;

-- Reserved (V2 multi-agent) --------------------------------------------
CREATE TABLE relationships (
  agent_id    TEXT NOT NULL,
  target_id   TEXT NOT NULL,
  note        TEXT,
  created_at  INTEGER NOT NULL,
  PRIMARY KEY (agent_id, target_id)
) STRICT;
```

Retrieval ranking (V1, no embeddings):

```
score = bm25(memories_fts) * recency_decay(now - last_access) * importance * scope_match(query.scope)
recency_decay(dt) = exp(-dt / TAU)            -- TAU = 14d default, configurable per scope
scope_match: 1.0 match | 0.5 compatible | 0.0 mismatch
```

Impact query:

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

### 4.5 Agent identity YAMLs

```yaml
# agents/coder-primary/core/soul.yaml
schema_version: 1
id: coder-primary
name: Coder Primary
role: persistent_coder
core_values: [correctness, safety, locality, traceability]
```

```yaml
# agents/coder-primary/core/heart.yaml
schema_version: 1
communication_style: concise
collaboration_style: solo
error_handling: fail_visible
```

```yaml
# agents/coder-primary/core/mind.yaml
schema_version: 1
planning_depth: shallow_task_level
risk_tolerance: low
validation_style: pessimist
```

```yaml
# agents/coder-primary/core/doctrine.yaml
schema_version: 1
must_never:
  - write_without_approval
  - bypass_orchestrator
  - exceed_tools_allowed
  - force_push_history
must_always:
  - produce_diff_before_apply
  - log_tool_events
  - attach_task_id_to_memory_writes
approval_required_for: [file_write, file_delete, patch_apply]
priorities:
  1: data_integrity
  2: user_safety
  3: correctness
  4: performance
```

No `mood`, `valence`, `trust`, `voice`, `personality_drift`, or `relationship` fields. Adding one violates `/docs/agent-definition.md`.

### 4.6 CLI surface (V1)

```
skycode scan <project>
skycode ask "<task>"
skycode diff <task-id>
skycode approve <diff-id>
skycode apply <approval-token>
skycode rollback <change-id>
skycode memory search "<query>"
skycode graph impact <path-or-symbol>
skycode model load <name>
skycode model verify <name>
skycode model bench <name>

skycode profile list
skycode profile show <profile>
skycode profile use <profile>
skycode profile test <profile> "<task>"
skycode profile compare <profile-a> <profile-b> "<task>"
skycode profile bench <profile>
skycode profile tune <model-name>
skycode profile export-results

skycode trust <path>
skycode untrust <path>
skycode trust list
skycode trust status <path>

skycode doctor
skycode version
skycode logs <task-id>
skycode audit <task-id>
skycode context <task-id>
skycode graph stats
skycode memory stats

skycode backup [--out <path>]
skycode restore <backup-file>
```

Exit codes: `0` ok, `2` validation failure, `3` policy denial, `4` patch conflict, `5` model load failure, `6` integrity failure, `7` trust failure, `8` profile/configuration failure.

### 4.7 Terminal/Tool Sandbox Policy (V1)

- Generic shell tool is **not** in V1. Only `git_status`, `git_diff`, `git_rev_parse`, `git_branch_show_current` invoke subprocesses, with fixed argv (no shell).
- All subprocess invocations:
  - `cwd` locked to the trusted project root resolved at task start. Path traversal rejected.
  - Hard `timeout` (default 30s, configurable per tool, max 120s).
  - Scrubbed environment: only `PATH`, `HOME`, `LANG`, `LC_*`, and a `SKYCODE_*` allowlist are inherited.
  - `stdin` closed.
  - `stdout`/`stderr` captured separately, redacted by the secrets scanner before logging.
  - No shell interpolation; no `bash -c`; no `&&`, `||`, `|`, `;`, backticks.
- Default V1 allowlist: `git status`, `git diff`, `git rev-parse`, `git branch --show-current`.
- Opt-in via `policy.yaml`: `cargo test`, `npm test`, `pytest -q`, `tsc --noEmit`.
- Categorically blocked regardless of policy: network commands; package install; destructive git (`push --force`, `reset --hard` user HEAD, `clean -fdx`, `branch -D`); paths outside project root; environment dumping; secret-revealing redirections.
- Every terminal event logs `tool_requested`; output secret-redacted before `tool_events.output_json` insert.

### 4.8 Secrets & Privacy Policy (V1)

Detection (V1 baseline):

- Filename heuristics: `.env`, `.env.*`, `*.pem`, `*.key`, `id_rsa*`, `id_ed25519*`, `*credentials*.json`, `*service-account*.json`, `*.pfx`, `*.p12`.
- Content patterns: AWS access keys, GCP service-account markers, GitHub PATs (`ghp_`, `gho_`, `ghs_`), Slack tokens (`xox[baprs]-`), JWTs (`eyJ[A-Za-z0-9_-]+\.eyJ[A-Za-z0-9_-]+\.[A-Za-z0-9_-]+`), high-entropy strings ≥40 chars adjacent to `key|token|secret|password|api[_-]?key`, RSA/EC PEM blocks, DB URLs with embedded creds.
- `.gitignore` honored.

Enforcement:

- Default-deny indexing of detected files (no memory rows, no FTS5, no symbol/import nodes; file node may carry `metadata_json.secret_skipped=true`).
- Pre-prompt redaction: every string entering a SkyCore request runs through the redactor; matches → `<<REDACTED:<kind>>>`; emits `secret_redacted` event.
- Pre-log redaction: tool output, errors, stack traces redacted before insertion.
- Memory writes: redactor runs on `content` and `tags`. Unredactable matches → `policy_denied`, abort write.
- Remote adapter (when explicitly enabled) is hard-blocked when context contains any field from a redacted file or matched a detection pattern.
- Override: `policy.yaml.allow_secret_paths: [path...]` plus CLI `--include-sensitive` for the specific task. Both must agree, else abort.

`skycode doctor` reports detection-rule version, `.gitignore` honored count, suspected-secret count.

### 4.9 Configuration Hierarchy

```
~/.skycode/
  config.yaml           # global config, schema_version
  models.yaml           # model registry, hot-reload, schema_version
  state/                # sqlite, agent_state snapshots
  cache/                # graph index cache, tokenizer cache
  keys/                 # signing key (mode 0600)
  logs/

<project>/.skycode/
  project.yaml          # project overrides (safe-settings only)
  policy.yaml           # restrictions only
  trust.yaml            # written by `skycode trust`

<project>/agents/coder-primary/core/
  soul.yaml | heart.yaml | mind.yaml | doctrine.yaml
```

Every file carries `schema_version: <int>`. Loader rejects unknown majors.

Override precedence (descending):

```
project/policy.yaml  >  project/project.yaml  >  ~/.skycode/config.yaml  >  built-in default
```

Strict rules:

1. `policy.yaml` may only **restrict** (`tools_allow`, `models_allow`, `paths_deny`, `network_deny`, `commands_deny`); never enables what global denies.
2. `project.yaml` may override only the safe-settings allowlist: editor preferences, project name, default model alias *within global allowed set*, default ctx size *not exceeding registry value*, scan globs, secret detection extensions. It may not flip safety, network, or remote-adapter settings.

Invariants:

- No model name is hardcoded in Rust.
- Default global config has remote adapters `enabled: false`.
- Invalid YAML / unknown fields / unknown enum values fail with file:line and non-zero exit.
- Hot-reload: `models.yaml`, `policy.yaml`. Restart-required: `config.yaml`, `project.yaml`, agent core files.

`skycode doctor` prints the resolved effective config and the source of each setting.

### 4.10 Project Trust Model

Trust record (`<project>/.skycode/trust.yaml`):

```yaml
schema_version: 1
canonical_path: /abs/path/to/project
git_remote_hash: sha256("origin\t<remote-url>") | null
created_at: 2026-05-06T12:00:00Z
last_seen_git_head: <oid> | null
policy_hash: sha256(policy.yaml bytes) | null
```

Modes:

- **Untrusted (default for any unseen path):**
  - read-only limited scan (file/folder graph nodes only, no symbol/import extraction, no FTS5 indexing of file contents);
  - no writes, no `apply_diff`, no `rollback`, no terminal tools;
  - no memory persistence except a session-scoped summary discarded at process exit;
  - remote adapter blocked unconditionally even if globally enabled;
  - secret scanner runs but redactions affect only the live session;
  - `skycode ask` forced to `output_contract = answer`.
- **Trusted:**
  - normal V1 behavior gated by approval, policy, secrets rules.

Invalidation:

- `git remote -v` change → `trust status` returns `stale`; revert to untrusted until `skycode trust <path>` is re-run.
- `policy.yaml` change → policy reloaded; trust record updates `policy_hash` only after re-trust if the change *widens* permissions vs the recorded hash. Pure restrictions auto-accept.
- Different `canonical_path` with same `git_remote_hash` creates a new trust record.

### 4.11 Pinned Versions

```yaml
# docs/PINS.yaml
schema_version: 1
rust_toolchain: "1.78.0"
llama_cpp:
  source: "github.com/ggerganov/llama.cpp"
  commit: "<pinned-sha>"
  build_flags: ["-DLLAMA_CUDA=ON", "-DLLAMA_NATIVE=OFF"]
tree_sitter:
  crate: "0.22.x"
  grammars: { python: "0.21.x", typescript: "0.21.x", rust: "0.21.x" }
sqlite_min: "3.41.0"
gguf_models:
  - alias: local-coder
    sha256: "<pinned-sha>"
  - alias: local-fallback
    sha256: "<pinned-sha>"
schemas:
  skycore_protocol:  "0.1"
  config:            1
  policy:            1
  models_registry:   1
  trust:             1
  migrations_head:   0007
```

`skycode model verify <name>`: registry parse; gguf path exists; sha256 matches PINS; GGUF metadata read; arch supported; tokenizer round-trips a known string; ctx_size ≤ training ctx unless RoPE configured/supported; declared runtime flags accepted by `llama-server --help` probe.

`skycode doctor` aggregates: pin file presence, runtime probe results, sqlite version + STRICT support + FTS5 support, signing key present, trust records summary, secret-rule version, network state, remote-adapter status, hardware class detection, effective profile (Patch 21).

### 4.12 Repo Safety & Git Isolation

- **Never mutate user HEAD.** Apply operates on `skycode/apply/<task-id>` created from current HEAD at apply time. Default returns user to original HEAD; `policy.yaml.checkout_after_apply: true` is the only way to switch.
- **Pre-apply ref recorded** in `applied_changes.pre_apply_git_ref`.
- **Blob hash verification** against `diff_proposals.base_blob_hashes_json`. Mismatch → `diff_apply_failed`, exit 4.
- **Dirty tree** rejects apply by default. Override: `policy.yaml.allow_dirty_apply: true` *and* CLI `--allow-dirty`; both must agree.
- **Rollback** resolves `applied_changes`, derives current state from `tool_events`, verifies the apply branch is still a descendant of `pre_apply_git_ref`, and resets only the apply branch back to that ref. Aborts with `rollback_failed` if unrelated commits exist on the apply branch.
- **`rolled_back_at` is not a column.** Rollback status is derived from `rollback_applied` events.

### 4.13 Context Budget

```yaml
context_budget:
  max_total_tokens:        20000
  max_raw_file_tokens:      4000
  max_memory_tokens:        4000
  max_graph_tokens:         6000
  max_instruction_tokens:   3000
  reserve_output_tokens:    4096
```

- Each slot is filled priority-ordered; surplus dropped (no mid-token truncation).
- Drops emit `context_budget_enforced` events with `dropped_kind` and `dropped_count`.
- `max_total_tokens + reserve_output_tokens ≤ ctx_size` of the chosen profile/model. If not, orchestrator falls back to `local-fallback` (Patch 5) and re-budgets.
- Context report (printed by `skycode ask`, queryable via `skycode context <task-id>`):

```
Context report (task <id>):
  memory_tokens / graph_tokens / file_snippet_tokens / instruction_tokens
  total_estimated / naive_baseline / reduction_pct
  output_reserved
  budget_drops: [{kind, count}]
```

The ≥50% reduction gate = `1 - (total_estimated / naive_baseline)` on the success-scenario task class.

### 4.14 Migrations & Backups

- `crates/skycode-memory/migrations/NNNN_<name>.sql`. `migrations_head` in `docs/PINS.yaml` = highest `NNNN`.
- Each migration is **idempotent** (`IF NOT EXISTS`), runs in a single `BEGIN…COMMIT`, tested from empty (`migrate fresh`) and from the previous head (`migrate upgrade`), never silently destructive (destructive changes require `--accept-destructive-migration` and emit `migration_destructive_applied`).
- `_skycode_migrations` records `version`, `applied_at`, `sha256`. Runtime refuses to start on tampered history.

Backup:

- Tarball, deterministic order, sha256 manifest.
- Includes: `state/skycode.sqlite` (online backup API), agent core YAMLs, `~/.skycode/config.yaml`, `~/.skycode/models.yaml`, public key material/key metadata only (private signing key excluded unless `--include-signing-key`), `<project>/.skycode/{project,policy,trust}.yaml`.
- Excludes graph cache by default (rebuildable); include with `--include-graph`.

Restore: refuses overwrite without `--force`; verifies manifest checksum; refuses on schema_version mismatch unless `migrations_head` covers the gap; runs migrations forward in a transaction.

### 4.15 Documentation Deliverables (canonical)

```
docs/architecture.md           # layer rules, allowed/forbidden crossings (§3)
docs/protocol.md               # SkyCore request/response, version policy
docs/agent-definition.md       # soul/heart/mind/doctrine minimal fields
docs/memory-system.md          # SQLite schema (§4.4)
docs/graph-system.md           # tree-sitter languages, node/edge taxonomy
docs/tool-system.md            # tool list, sandbox policy (§4.7)
docs/policy-system.md          # config hierarchy + override rules (§4.9)
docs/model-runtime.md          # llama.cpp flags, registry, hardware classes (§5)
docs/configuration.md          # full config schema (§4.9)
docs/security.md               # secrets scanner, redaction, trust (§4.8 / §4.10)
docs/runtime-tuning.md         # tuning profiles (§5.8 / Patch 21)
docs/testing-lab.md            # tuning runs, comparisons (§5.8 / Patch 21)
docs/roadmap.md                # phases mirror §6
docs/v1-success-criteria.md    # §11
```

CI: `docs::all_canonical_files_exist`, `docs::plan_does_not_contradict_docs`.

---

## 5. Inference Runtime — Low-VRAM MoE Configuration

### 5.1 Default model class: 35B MoE (Qwen3-class, A3B activation)

V1 defaults to a Qwen3-class 35B MoE (A3B activation) for hosts in the §HARDWARE TARGET tier. MoE is preferred over a dense 7–13B because the *active* parameters per token are small enough to keep the always-active path on a 6GB GPU while expert blocks live in RAM. A dense model of comparable quality would not fit at all. (Adopted: ChatGPTV2 / DeepSeekV2 model-class direction; rationale extended.)

### 5.2 Forbidden default: naive `-ngl` half-and-half split

Splitting layers across PCIe with `-ngl <half>` for a MoE model thrashes the bus (~3 tok/s on the §HARDWARE TARGET). Forbidden as a default.

### 5.3 Required default flags for low-VRAM target

```
--n-cpu-moe <N>          # pin expert blocks to RAM; tune per §5.4
--no-mmap                # prevent OS paging of expert weights from disk
--mlock                  # lock weights resident; verify post-launch
--cache-type-k q8_0      # KV cache K, baseline
--cache-type-v q8_0      # KV cache V, baseline
                         # >128K via K=q4_0, V=q3_* turbo only when build supports;
                         # verify against `llama-server --help` per build
--ctx-size 131072        # 128K target window
# speculative decoding: OFF by default (MoE expert thrashing; SSM-layer conflicts)
```

Container caveats: LXC must permit mlock; Docker/Podman need `--cap-add IPC_LOCK` and `--ulimit memlock=-1:-1`. `mlock` silently ignored if kernel rejects — verify via `VmLck` (Linux) or locked-pages probe (Windows).

The runtime never claims a flag it could not verify: `--cache-type-k`, `--cache-type-v`, `--n-cpu-moe`, `--mlock`, `--no-mmap` are probed at startup. Unsupported → explicit startup error.

### 5.4 Tuning loop

```
1. load model with default flags from registry entry × profile
2. measure: VRAM, RAM, tok/s, ctx achieved, mmap, mlock
3. VRAM < 90% → decrement --n-cpu-moe by 1, restart, goto 2
4. VRAM > 96% → increment --n-cpu-moe by 1, restart, goto 2
5. expand context via cache-type quantization until target ctx reached
6. OOM at target ctx → increment --n-cpu-moe by 1, goto 5
7. write tuned values back to the registry entry as a derived profile
```

`skycode profile tune <model-name>` runs this loop and writes the result as a new tuned profile (Patch 21).

### 5.5 Model registry YAML (hot-reloadable)

```yaml
schema_version: 1
models:
  - name: local-coder
    runtime: llama_cpp
    gguf_path: ~/.skycode/models/qwen3-35b-a3b.Q4_K_M.gguf
    n_cpu_moe: 41
    cache_type_k: q8_0
    cache_type_v: q8_0
    ctx_size: 131072
    mlock: true
    no_mmap: true
    speculative: false
    expected_vram_gb: 5.7
    expected_tok_s: 17.0
    hardware_class: low_vram_6gb
    strengths: [code_edit, refactor, plan]
    profiles_supported: [fast, deep, precise, creative]
    default_profile: precise

  - name: local-fallback
    runtime: llama_cpp
    gguf_path: ~/.skycode/models/qwen3-coder-7b.Q4_K_M.gguf
    n_cpu_moe: 0
    cache_type_k: q8_0
    cache_type_v: q8_0
    ctx_size: 32768
    mlock: true
    no_mmap: true
    speculative: false
    expected_vram_gb: 4.8
    expected_tok_s: 25.0
    hardware_class: low_vram_6gb
    strengths: [classify, short_answer]
    profiles_supported: [fast, precise]
    default_profile: fast

  - name: remote-strong
    runtime: openai_compatible
    enabled: false
    base_url: ""
    api_key_env: ""
    hardware_class: any
    profiles_supported: []
```

### 5.6 Hardware classes

- `low_vram_6gb`: GTX 1060, RTX 2060 mobile, 6GB RTX-30 — `--n-cpu-moe` high, KV q8 baseline, ctx 128K via cache-q.
- `mid_vram_12gb`: RTX 3060 12GB / 4070 mobile — `--n-cpu-moe` lower, KV q8, ctx 128K f16 viable.
- `high_vram_24gb`: RTX 3090 / 4090 — `--n-cpu-moe 0`, KV f16, ctx 128K+ trivial.
- `cpu_only`: no compatible GPU — all on CPU, smaller fallback preferred, ctx 32K.

Selecting the wrong class is a `skycode model load` warning, never silent fallback.

### 5.7 Tiered hardware acceptance (replaces §5 hard targets)

**Required minimum (V1 release blocker, all classes):**

- Local offline inference completes a SkyCore round-trip.
- Configured GGUF model loads.
- 4096-token decode without crash.
- ctx ≥ 32K verified.
- No silent remote fallback.
- `skycode model bench` reports true VRAM/RAM/tok-s/ctx/mmap/mlock/OOM.
- Startup probes runtime flags; refuses to claim what it could not verify.

**Target benchmark (§HARDWARE TARGET sign-off, low_vram_6gb):**

- 35B MoE (Qwen3-class, A3B), ctx ≥ 128K via cache quant, ≥ 15 tok/s, VRAM ≤ 5.9GB, mlock active (≥90% locked), mmap off (verified), OOM count 0 in 4096-token decode, speculative off.

**Acceptable fallback (when target hardware not present):**

- Auto-switch to `local-fallback`, ctx ≥ 32K, ≥ 8 tok/s.
- No remote unless explicitly enabled by user.
- `skycode doctor` records the reason.
- mlock/no-mmap remain *recommended* on the fallback path; container/host denial degrades performance but does not block release.

### 5.8 Bench command

```
skycode model bench <name>
```

Reports per registry entry: tok/s prompt eval (first-token latency); tok/s decode (steady-state); peak VRAM during 4096-token decode; peak resident RAM; achieved ctx (loaded vs requested); mmap status (must be `no` when `no_mmap: true`); mlock status (resident bytes vs total weights — verified, not trusted); OOM events; spec-decode status (must be `off` by default); active profile (Patch 21).

Required for Phase 5 sign-off on the §HARDWARE TARGET tier.

---

## 6. Phased Execution (12 weeks)

### Phase 0 — Canonical Freeze (Week 0–1)

**Goal.** Lock `/docs`. Initialize Rust workspace. No product behavior.

**Deliverables.**
- This document committed; supersedes `docs/PLAN.md` (kept as history).
- Rust workspace skeleton:
  ```
  skycode/
    crates/
      skycode-core/         # SkyCore types, approval token, errors
      skycode-protocol/     # request/response (de)serialization
      skycode-memory/       # SQLite migrations, FTS5, repo
      skycode-graph/        # tree-sitter parsers, graph DDL, queries
      skycode-tools/        # read/list/search/git/diff/apply/rollback
      skycode-inference/    # llama.cpp ffi/subprocess + remote adapter trait
      skycode-policy/       # secrets scanner, sandbox, trust enforcer
      skycode-runtime/      # agent runtime, no fs/model/tools deps
      skycode-orchestrator/
      skycode-cli/
    docs/
    agents/coder-primary/...
  ```
- `Cargo.toml` deny rules for forbidden cross-crate dependencies.
- Migration `0001_init.sql` matching §4.4.
- SkyCore Rust types matching §4.1, §4.2 with `#[serde(deny_unknown_fields)]`.
- Approval token sign/verify code; signing key bootstrapped on first run.

**Boundary crossings.** None at runtime.

**Risk / control.** Risk: contradictions in `/docs` propagate. Control: a `Phase 0 freeze` issue lists every contradiction and resolution; closed before Phase 1.

**Exit gates.**
- Zero contradictions in `/docs` (or every contradiction documented).
- Every V1 feature mapped to one canonical layer.
- Every write-capable operation has a named approval gate.
- No V1 task in this plan requires the network.
- `cargo test -p skycode-core -p skycode-protocol` passes for round-trip serialization.

### Phase 1 — Safe Tool Spine (Week 1–3)

**Goal.** Safe edit pipeline on real repos with no LLM.

**Deliverables.**
- Read tools: `read_file`, `list_dir`, `search_project`, `git_status`, `git_diff`.
- Write pipeline: `create_diff` (persists into `diff_proposals`), `apply_diff(diff_id, token)` (validates per §4.3, applies onto isolated branch per §4.12), `rollback(change_id)`.
- Approval token (ed25519, TTL=300s, single-use via `approval_tokens_used`).
- Append-only `tool_events` event-sourced; `applied_changes` immutable.
- CLI: `diff`, `approve`, `apply`, `rollback`.

**Boundary crossings.** `CLI → Orchestrator → Tools → filesystem/git`. **Risk:** unauthorised mutation. **Control:** write fns private; `apply_diff` is sole public mutator; token validation is first statement.

**Exit gates.**
- 50 simulated edit cycles produce zero unapproved writes per audit.
- Red-team test cannot find a public write path other than `apply_diff`.
- Multi-file rollback verified on a real repo.
- Every tool call produces queryable events.
- `tool_events`/`diff_proposals`/`applied_changes`/`approval_tokens_used` `UPDATE`/`DELETE` triggers fire.

### Phase 2 — Memory + Graph V1 (Week 3–5, overlaps Phase 1 from Week 3)

**Goal.** Retrieval substrate.

**Deliverables.**
- All migrations from §4.4 applied.
- Memory write API: scope-tagged inserts with `task_id`.
- Memory retrieval API ranked by `bm25 * recency_decay * importance * scope_match`.
- Project scanner with tree-sitter (Python, TypeScript, Rust default; others auto-detected and warned).
- Graph nodes/edges per §4.4.
- `skycode graph impact` recursive CTE.
- Incremental rescan on mtime+size; full rescan via `skycode scan --force`.
- `.git/HEAD` watcher (Known Problems §9 mitigation).

**Boundary crossings.** `Orchestrator → Memory/Graph`; scanner read-only. **Risk:** OOM on large repos; FTS5 ranking degradation past ~100k rows. **Control:** scanner streams; FTS5 external-content; hard `LIMIT` and `bm25` cutoff at retrieval time.

**Exit gates.**
- Scan persists across restart.
- Memory retrieval scoped (project-only, agent-only).
- Graph impact correct on ≥3 real refactors.
- Memory retrieval p95 <200ms at 10k rows.
- No vector DB / embeddings / remote service used.
- `skycode-memory` and `skycode-graph` have no `skycode-inference` dep.

### Phase 3 — Local Inference + SkyCore (Week 5–7)

**Goal.** Local llama.cpp behind SkyCore. Hot-reloadable registry. Low-VRAM MoE defaults.

**Deliverables.**
- `skycode-inference` crate: llama.cpp subprocess wrapper as V1 default (FFI as upgrade path); §5.3 flags with §5.6 hardware-class defaults.
- mlock verification post-launch.
- Cache-type flag presence detection per build.
- Streaming token interface.
- Model registry loader (`models.yaml`); file-watcher hot reload.
- `skycode model load`, `skycode model verify`, `skycode model bench`.
- SkyCore serializer/deserializer with `deny_unknown_fields`.
- Optional remote adapter behind a `runtime: openai_compatible` registry entry; **disabled by default**.
- Tuning loop wired into `skycode model bench` and `skycode profile tune`.

**Boundary crossings.** `Orchestrator → SkyCore client → Inference Runtime → Models`. (Note: SkyCore client lives in orchestrator, not in agent runtime.) **Risk:** provider format bleed; mlock silent failure; flag drift. **Control:** boundary-bleed integration test; mlock verifier; flag probe.

**Exit gates.**
- Required minimum tier passes (§5.7) on the build host.
- Identical SkyCore request/response shape across local and (when enabled) remote adapter.
- Missing model → explicit error, no remote fallthrough.
- Registry hot-reload verified.
- Target benchmark passes on §HARDWARE TARGET if available.

### Phase 4 — Persistent Coder Agent (Week 7–10)

**Goal.** End-to-end V1 product loop on a single agent.

**Deliverables.**
- `coder-primary` instantiated from §4.5.
- `agent_state` persisted; reloaded on start.
- Orchestrator pipeline (mirrors §3 Phase 4 diagram):
  ```
  classify task
    → load coder-primary identity                  [orchestrator → runtime]
    → agent renders AgentIntent                    [runtime]
    → resolve context_refs (memory + graph)        [orchestrator → memory/graph]
    → enforce context budget (§4.13)               [orchestrator]
    → secret/redaction pass (§4.8)                 [orchestrator]
    → trust enforcement (§4.10)                    [orchestrator]
    → build SkyCore request                        [orchestrator]
    → invoke Inference Runtime via SkyCore client  [orchestrator → inference]
    → receive normalized SkyCore response          [inference → orchestrator]
    → agent parses response into DiffProposal      [orchestrator → runtime]
    → orchestrator stores diff in diff_proposals   [orchestrator → memory]
    → CLI shows diff; awaits approve/apply         [orchestrator → cli]
    → on approve: sign token bound to diff_id      [orchestrator]
    → on apply: validate + load + apply + log      [orchestrator → tools]
  ```
- Decision writer per applied change.
- CLI `skycode ask` proposes only; never silently applies.
- `skycode profile use <profile>` honored end-to-end (Patch 21).

**Boundary crossings.** All allowed crossings exercised. **Risk:** agent attempting direct tool/model access; project context bleed. **Control:** crate-level deps + arch tests; `project_id` is a required argument in retrieval APIs.

**Exit gates.**
- Agent recalls a decision from session 1 in session 3 after two restarts.
- One real safe edit completed offline on a 50k-line repo.
- All edits route diff → approval → apply → log; 0 unapproved writes.
- Context tokens for the success scenario ≤ 50% naive baseline.
- Exactly one agent in runtime (asserted at startup).
- `model_invoked` events carry `profile_name`.

### Phase 5 — Hardening + Lightweight Router + Testing Lab (Week 10–12)

**Goal.** Ship V1. Reviewer agent does not appear in any framing.

**Hardening (release-blocker).**
- End-to-end regression suite covering every prior gate.
- Failure-mode tests: missing model; expired/double-spent/scope-mismatched/diff-id-mismatched token; patch conflict (base-blob mismatch); process kill mid-task; rollback when git state drifted; SQLite locked; FTS5 corruption recovery; registry/policy YAML invalid; mlock denied (container); `migrate fresh` and `migrate upgrade`.
- Offline demo: full V1 with network disabled at OS level.
- Benchmark: graph-aware retrieval ≥50% reduction vs naive file-dump.
- Hardware-class bench: target tier on §HARDWARE TARGET; required-minimum on build host.
- Operator docs: CLI reference, model setup, registry tuning, testing-lab walkthrough.

**Lightweight router (additive).**
- Task classifier maps `goal` → `{classify, short_answer, code_edit, refactor, plan}` via heuristic keyword + structural rules (no second model).
- Router maps task class → registry entry × profile → fallback chain (`local-primary → local-fallback → explicit failure`). No silent remote.
- Telemetry to `tool_events`: latency, tokens, model used, profile used, fallback fired.

**Testing Lab (Patch 21).**
- `skycode profile bench`, `compare`, `tune`, `export-results` all functional.
- `tuning_runs` populated by every test/compare/bench.

**Boundary crossings.** None new. **Risk:** release with hidden unsafe path; remote adapter accidentally enabled in tests. **Control:** release-block on any gate failure; CI offline test asserts no socket open; registry-default test asserts `enabled: false` on remote entries.

**Exit gates.**
- Zero unapproved writes in full suite.
- Offline demo passes end-to-end.
- ≥50% context-token reduction confirmed.
- Tool + decision logs reconstruct every applied change.
- SQLite sufficient under measured workload (read p95 <200ms, write p95 <50ms at 100k memory rows).
- Required-minimum hardware tier passes; target tier passes on §HARDWARE TARGET.
- Router selects correct class on ≥9/10 hand-labelled samples; fallback fires on simulated primary failure.
- Profile bench/compare/tune workflows write `tuning_runs` rows correctly.

---

## 7. Universal Phase Gate Checklist

Applied at every phase close. A phase cannot close until every line is `pass`.

| Gate          | Criterion                                                                  |
|---------------|----------------------------------------------------------------------------|
| Safety        | Zero unapproved writes in full audit for the phase.                        |
| Persistence   | All state required by the phase survives clean restart.                    |
| Traceability  | Every event queryable by `task_id` and timestamp.                          |
| Boundary      | No layer boundary crossed outside §3 allowed list.                         |
| Quality       | Phase-specific numeric threshold met (§6).                                 |
| Trust         | Untrusted-mode invariants hold for any non-trusted path used in tests.     |
| Privacy       | Secret scanner runs; no unredacted secret reaches memory, prompt, or log.  |
| Pinning       | `docs/PINS.yaml` matches actual toolchain/grammars/sqlite/migrations head. |

---

## 8. Test Plan

### Unit
- `approval_token::create_then_verify_succeeds`
- `approval_token::expired_rejected`
- `approval_token::scope_mismatch_rejected`
- `approval_token::diff_id_mismatch_rejected`
- `approval_token::tampered_signature_rejected`
- `approval_token::single_use_replay_rejected_via_pk_violation`
- `approval_token::tid_collision_attack_rejected`
- `approval_token::diff_id_resolution_via_diff_proposals`
- `approval_token::regression_step_order_matches_spec`
- `tool_policy::write_without_token_rejected`
- `tool_policy::read_without_token_allowed`
- `skycore::request_roundtrip_serde`
- `skycore::response_roundtrip_serde`
- `skycore::deny_unknown_fields_rejects_provider_shape`
- `memory::ranking_keyword_recency_importance_scope`
- `graph::node_id_sha256_stable`
- `graph::edge_recursive_impact_correct`

### Integration
- `safe_edit::diff_approve_apply_log_rollback_single_file`
- `safe_edit::diff_approve_apply_log_rollback_multi_file`
- `safe_edit::diff_proposals_immutable`
- `safe_edit::approve_unknown_diff_rejected`
- `safe_edit::approve_expired_diff_rejected`
- `safe_edit::apply_verifies_base_blob_hashes`
- `safe_edit::concurrent_external_edit_yields_patch_conflict`
- `scan_then_graph_impact_then_context_build`
- `cli::ask_produces_diff_no_silent_apply`
- `restart::agent_recalls_decision_session3_two_restarts`
- `fts5::scoping_project_and_agent_isolation`
- `fts5::p95_under_200ms_at_10k_rows`
- `audit::state_reconstructs_from_events`
- `git::apply_creates_dedicated_branch`
- `git::apply_does_not_move_user_head_by_default`
- `git::apply_rejects_dirty_tree_by_default`
- `git::rollback_aborts_when_unrelated_commits_on_apply_branch`
- `git::applied_changes_immutable`
- `git::rollback_status_derived_from_events`

### Offline
- `offline::full_v1_flow_with_network_disabled`
- `offline::ci_default_config_opens_no_sockets`

### Regression / architectural
- `arch::cli_only_depends_on_orchestrator`
- `arch::cli_has_no_dep_on_inference`
- `arch::cli_has_no_dep_on_memory`
- `arch::runtime_has_no_dep_on_tools`
- `arch::runtime_has_no_dep_on_inference`
- `arch::runtime_has_no_std_fs_net_process`
- `arch::runtime_only_reads_memory_graph`
- `arch::tools_no_public_write_outside_apply_diff`
- `regression::provider_format_never_reaches_agent_runtime`
- `regression::provider_format_never_reaches_orchestrator_or_cli`
- `regression::tool_events_update_delete_blocked_by_trigger`

### Failure-mode
- `failmode::missing_model_explicit_error`
- `failmode::expired_token_rejected`
- `failmode::double_spent_token_rejected`
- `failmode::patch_conflict_after_external_edit`
- `failmode::process_kill_mid_task_recovers_state`
- `failmode::rollback_when_git_state_changed`
- `failmode::sqlite_locked_retries_then_fails`
- `failmode::registry_yaml_invalid_explicit_error`
- `failmode::mlock_denied_in_container_reports_clearly`
- `failmode::migration_destructive_requires_flag_and_logs_event`
- `failmode::migration_tamper_detection_via_sha256`

### Hardware-class bench
- `bench::required_minimum::offline_roundtrip_completes`
- `bench::required_minimum::ctx_at_least_32k_verified`
- `bench::required_minimum::startup_probes_llama_cpp_flags`
- `bench::target::low_vram_6gb::tok_s_ge_15_at_ctx_131072`
- `bench::target::low_vram_6gb::vram_le_5_9_gb`
- `bench::target::low_vram_6gb::mlock_actually_active`
- `bench::target::low_vram_6gb::mmap_disabled_actually`
- `bench::target::low_vram_6gb::oom_count_zero_during_4096_decode`
- `bench::fallback::degrades_to_local_fallback_not_remote`
- `bench::all_classes::tok_s_no_regression_vs_baseline_5pct`

### Sandbox / Secrets / Trust / Config / Pins / Backup / Context / Profiles / Diagnostics
- `sandbox::cwd_locked_to_project_root`
- `sandbox::path_traversal_rejected`
- `sandbox::timeout_enforced`
- `sandbox::env_scrubbed`
- `sandbox::no_shell_interpolation`
- `sandbox::default_allowlist_minimal`
- `sandbox::policy_can_only_restrict_not_widen`
- `sandbox::blocked_categories_rejected_even_with_policy`
- `secrets::env_files_not_indexed_by_default`
- `secrets::regex_set_redacts_known_token_shapes`
- `secrets::gitignore_honored`
- `secrets::pre_prompt_redaction_logs_secret_redacted_event`
- `secrets::tool_output_redacted_before_log`
- `secrets::memory_write_blocked_on_unredactable_match`
- `secrets::remote_adapter_blocked_when_context_contains_secret_origin`
- `secrets::cli_and_policy_must_both_agree_for_override`
- `trust::default_path_is_untrusted`
- `trust::untrusted_blocks_writes_and_terminal`
- `trust::untrusted_forces_output_contract_answer`
- `trust::untrusted_blocks_remote_unconditionally`
- `trust::trust_creates_record_with_canonical_path_and_remote_hash`
- `trust::remote_change_marks_stale`
- `trust::policy_widening_requires_re_trust`
- `config::loader_rejects_unknown_fields`
- `config::policy_cannot_widen_global`
- `config::project_cannot_flip_safety_settings`
- `config::registry_hot_reload_no_restart`
- `config::policy_hot_reload_no_restart`
- `config::default_global_remote_disabled`
- `config::doctor_emits_effective_resolution`
- `pins::pinned_versions_loaded_from_PINS_yaml`
- `pins::skycode_version_emits_pins`
- `pins::doctor_reports_runtime_probe_results`
- `pins::model_verify_checksum_mismatch_fails`
- `pins::model_verify_unsupported_flag_fails_explicitly`
- `pins::model_verify_ctx_exceeds_training_without_rope_fails`
- `migrate::fresh_from_empty`
- `migrate::upgrade_from_previous_head`
- `backup::manifest_checksums_verify`
- `backup::excludes_signing_key_by_default`
- `restore::refuses_overwrite_without_force`
- `restore::runs_forward_migrations_in_txn`
- `context::budget_enforced_per_slot`
- `context::drops_emit_event`
- `context::report_printed_by_skycode_ask`
- `context::report_queryable_by_task_id`
- `context::baseline_calculation_matches_referenced_files_full_size`
- `context::reduction_pct_at_least_50_on_success_scenario`
- `cli::doctor_runs_without_network`
- `cli::logs_renders_event_stream_human_readable`
- `cli::audit_emits_machine_readable_event_sequence`
- `cli::context_returns_per_task_report`
- `cli::graph_stats_reports_kind_counts_and_last_scan`
- `cli::memory_stats_reports_scope_counts_and_p95`
- `cli::diagnostics_never_mutate_audit_tables`
- `profile::list_profiles_from_registry`
- `profile::use_profile_updates_effective_runtime_config`
- `profile::profile_cannot_enable_remote`
- `profile::profile_cannot_weaken_policy`
- `profile::profile_test_logs_tuning_run`
- `profile::profile_compare_records_two_runs`
- `profile::doctor_shows_effective_profile`
- `profile::model_invoked_event_contains_profile_name`
- `profile::invalid_profile_setting_fails_clear`
- `profile::tune_writes_best_safe_profile_to_registry`
- `release_gate::all_patch_acceptance_criteria_pass`

---

## 9. Known Problems Pre-Identified

- **PCIe thrashing on naive offload** → `--n-cpu-moe` defaults; tuning loop targets 95% VRAM.
- **OS paging of experts under pressure** → `--no-mmap` + `--mlock` defaults; `bench::target::low_vram_6gb::mlock_actually_active`.
- **KV cache flag drift** → startup probes `llama-server --help`; runtime refuses unverified claims.
- **mlock silently failing in containers** → measured `VmLck` (Linux) / locked-pages (Windows); fails on target tier, warns on fallback tier.
- **tree-sitter grammar gaps** → V1 commits to Python/TypeScript/Rust; other languages emit file/folder nodes only with a clear warning.
- **FTS5 ranking degradation past ~100k rows** → `bm25 * recency_decay * importance * scope_match`; hard `LIMIT`; benchmark; embeddings remain post-V1 only after measured failure persists.
- **Approval token replay** → §4.3 step 9 atomic insert into `approval_tokens_used` (PK violation).
- **Patch conflict on apply after concurrent edit** → `base_blob_hashes_json` verified at apply time; exit 4 + `diff_apply_failed`.
- **Agent context bleed across projects** → `project_id` required in retrieval APIs; unscoped queries don't typecheck; `fts5::scoping_project_and_agent_isolation`.
- **SkyCore version skew** → `skycore_version` required; orchestrator rejects mismatched majors.
- **Remote adapter accidentally enabled** → `offline::ci_default_config_opens_no_sockets`; registry-default test.
- **Graph index staleness after external git ops** → `.git/HEAD` and `.git/refs` watcher; staleness flag in `skycode graph impact`.
- **Untrusted repo content drives behavior** → trust enforcer blocks writes/terminal/remote; output forced to `answer`.
- **Secrets entering prompt/memory/log** → secret scanner + redactor + memory-write block + remote adapter block; `secret_redacted` events.
- **Profile broadens policy** → profile loader rejects fields outside the tuning whitelist (Patch 21).

---

## 10. Post-V1 Backlog (gate-locked)

- **Reviewer agent.** Trigger: all V1 gates pass + 2 weeks of stable V1 use.
- **Remote model fallback enabled by user config.** Trigger: V1 ships + labelled benchmark shows ≥10% quality lift on `refactor`/`plan` from remote, with explicit user opt-in.
- **Manager / architect agents.** Trigger: reviewer stable for 4 weeks; multi-agent contract added to SkyCore.
- **Tauri UI.** Trigger: reviewer in production; CLI usability identifies a UI-amenable flow.
- **Vector embeddings.** Trigger: FTS5 + recency + importance + scope ranking measurably fails recall@10 on a labelled benchmark *after* tuning the rank function.
- **Multi-agent orchestration.** Trigger: reviewer + coder pair stable.
- **Active relationship memory.** Trigger: multi-agent in production. The `relationships` table remains dormant until then.

Never: swarm consensus, personality drift, emotional valence, voice, multimodal, AI-civilization framing.

---

## 11. V1 Success Definition

A developer on a §HARDWARE TARGET workstation, network disabled, runs:

```
$ skycode trust .
$ skycode doctor
$ skycode model verify local-coder
$ skycode scan .
$ skycode profile list
$ skycode profile bench precise
$ skycode profile use precise
$ skycode ask "extract auth logic in src/auth/* into a new src/services/auth_service.* module \
                and update call sites"
```

Required outcomes (every line is a release blocker):

1. `skycode trust .` records `trust.yaml` with canonical path, git remote hash, policy hash.
2. `skycode doctor` returns success: pins loaded; runtime probe ok; sqlite ≥ pinned min; FTS5 + STRICT supported; signing key present; network disabled; remote-adapter disabled; secret-rule version reported; effective profile reported.
3. `skycode model verify local-coder` returns success: file checksum matches PINS; GGUF metadata reads; declared flags accepted by runtime probe; ctx within training/RoPE limits.
4. `skycode scan .` builds graph + memory; secret scanner skips secret-classified files and emits `secret_redacted` events.
5. `skycode profile bench precise` writes a `tuning_runs` row with first-token latency, decode tok/s, peak VRAM, peak RAM, ctx achieved, OOM count.
6. `skycode profile use precise` updates effective profile; subsequent `model_invoked` events carry `profile_name=precise`.
7. `skycode ask` produces a `diff_proposal`:
   - context drawn only from `memory:*` and `graph:*` refs;
   - context-budget report shows `total_estimated ≤ 50% × naive_baseline`;
   - diff persisted to `diff_proposals` (immutable);
   - `tool_events` contains `tool_requested`, `model_invoked` (with `profile_name`), `diff_proposed`, `context_budget_enforced` events.
8. `skycode approve <diff-id>` returns a signed token (UUIDv4 `tid`, ed25519, scope=`apply_diff`, bound to `did` and `tsk`, TTL=300s).
9. `skycode apply <token>` validates per §4.3 ordering, atomically inserts into `approval_tokens_used`, loads the immutable diff from `diff_proposals`, verifies `base_blob_hashes_json`, applies onto `skycode/apply/<task-id>` without moving user HEAD, writes an `applied_changes` row, and appends `diff_approved` + `diff_applied` events.
10. A `decisions` row exists with `outcome=approved`, `summary`, `rationale`, `context_refs`.
11. After two full process restarts and three sessions later, `skycode ask "why did we extract AuthService?"` cites the prior decision, retrieved from `memories` + `decisions` via FTS5.
12. Replay: re-running `skycode apply <token>` is rejected at step 9 (PK violation in `approval_tokens_used`). External edit then re-apply yields `diff_apply_failed` exit 4.
13. `skycode rollback <change-id>` reverts the apply branch to `pre_apply_git_ref` without touching user HEAD; rollback status derives from `rollback_applied` events.
14. `skycode audit <task-id>` reconstructs the full lifecycle from `tool_events` alone.
15. `skycode context <task-id>` re-emits the budget report with stable numbers.
16. All architectural dependency tests pass.
17. The full run completes with the network disabled at the OS level. `offline::ci_default_config_opens_no_sockets` passes.

Hardware acceptance, tiered (§5.7):

- **Required minimum (any class, V1 build):** offline round-trip; ctx ≥ 32K; no silent remote; runtime never claims unverified flags.
- **Target benchmark (§HARDWARE TARGET sign-off):** Qwen3-class 35B MoE; ctx ≥ 128K via cache quant; ≥ 15 tok/s; VRAM ≤ 5.9GB; mlock active (≥90% locked); mmap off (verified); OOM count 0; speculative off.
- **Acceptable fallback (when target hardware not present):** auto-switch to `local-fallback`; ctx ≥ 32K; ≥ 8 tok/s; no remote unless explicitly enabled; doctor records reason.

That is V1. Reviewer, multi-agent, UI, embeddings, remote-default, voice, multimodal, swarm, personality drift, emotional valence, active relationship memory — all post-V1, gate-locked per §10.

---

## Appendix A — Patch Index

This master plan integrates the improvement patches issued after `docs/PLAN.md`. Each patch is incorporated where applicable; the original patch text is preserved in the commit history.

- **Patch 1** — Agent / Orchestrator / SkyCore boundary corrected (orchestrator owns SkyCore call). Integrated into §3, §4.3, §6 Phase 4.
- **Patch 2** — `diff_proposals` immutable storage. Integrated into §4.4 and §4.3.
- **Patch 3** — `tool_events` event-sourced (no mutable status). Integrated into §4.4.
- **Patch 4** — `approval_tokens_used` atomic replay defense. Integrated into §4.4 and §4.3.
- **Patch 5** — Tiered hardware acceptance (required minimum / target / fallback). Integrated into §5.7 and §11.
- **Patch 6** — Terminal/tool sandbox policy. Integrated into §4.7.
- **Patch 7** — Secrets and privacy scanner. Integrated into §4.8 and `skycode-policy` crate.
- **Patch 8** — Configuration hierarchy. Integrated into §4.9.
- **Patch 9** — Project trust model. Integrated into §4.10.
- **Patch 10** — Dependency / version pinning. Integrated into §4.11 and §4.6 (`doctor`, `version`, `model verify`).
- **Patch 11** — Repo safety / git isolation, `applied_changes`. Integrated into §4.4 and §4.12.
- **Patch 12** — Context-budget contract. Integrated into §4.13.
- **Patch 13** — Observability and diagnostics. Integrated into §4.6.
- **Patch 14** — Migration and backup strategy. Integrated into §4.4 (`_skycode_migrations`) and §4.14.
- **Patch 15** — Documentation deliverables. Integrated into §4.15.
- **Patch 16** — Acceptance criteria update. Integrated into §11.
- **Patch 21** — Runtime Tuning Profiles & Testing Lab. See Appendix B.

Patch numbers 17–20 are reserved.

---

## Appendix B — Patch 21: Runtime Tuning Profiles & Testing Lab

### Purpose

Allow the user to experiment with model/runtime behavior without editing Rust. Profiles let users compare configurations, benchmark trade-offs, and discover the best settings for their hardware and task class — while the safety surface stays fixed.

### B.1 Tuning profiles in the model registry

`~/.skycode/models.yaml` carries a `profiles:` map at the top level. Profiles are *runtime tuning only*; they cannot touch policy, approval, tools, secrets, audit, or remote-adapter state.

```yaml
schema_version: 1
profiles:
  fast:
    description: "Low-latency coding and short answers"
    ctx_size: 32768
    temperature: 0.2
    top_p: 0.85
    top_k: 40
    repeat_penalty: 1.1
    cache_type_k: q8_0
    cache_type_v: q8_0
    n_cpu_moe: auto
    speculative: false

  deep:
    description: "Long-context architecture and project reasoning"
    ctx_size: 131072
    temperature: 0.35
    top_p: 0.9
    top_k: 50
    repeat_penalty: 1.08
    cache_type_k: q4_0
    cache_type_v: q4_0
    n_cpu_moe: tuned
    speculative: false

  precise:
    description: "Deterministic edits and safe refactors"
    ctx_size: 65536
    temperature: 0.1
    top_p: 0.75
    top_k: 20
    repeat_penalty: 1.15
    cache_type_k: q8_0
    cache_type_v: q8_0
    n_cpu_moe: tuned
    speculative: false

  creative:
    description: "Brainstorming, planning, naming, alternatives"
    ctx_size: 32768
    temperature: 0.75
    top_p: 0.95
    top_k: 80
    repeat_penalty: 1.03
    cache_type_k: q8_0
    cache_type_v: q8_0
    n_cpu_moe: auto
    speculative: false
```

`n_cpu_moe: auto` instructs the runtime to use the `low_vram_6gb`-class default at load time. `n_cpu_moe: tuned` requires a prior `skycode profile tune <model-name>` run; otherwise the loader rejects the profile. Each registry `models[*]` entry carries `profiles_supported: [...]` and a `default_profile`.

Profile schema (closed allowlist; loader rejects unknown fields):

```
description, ctx_size, temperature, top_p, top_k, repeat_penalty,
cache_type_k, cache_type_v, n_cpu_moe, speculative,
mirostat, mirostat_tau, mirostat_eta, presence_penalty, frequency_penalty
```

Forbidden in profiles (loader rejects with `policy denial`):

```
remote_url, api_key_env, enabled, base_url,
tools_allow, tools_deny, approval_required_for, must_never, must_always,
allow_secret_paths, allow_dirty_apply, audit_disabled, log_disabled,
trust_override, sandbox_disabled
```

### B.2 CLI

```
skycode profile list
skycode profile show <profile>
skycode profile use <profile>
skycode profile test <profile> "<task>"
skycode profile compare <profile-a> <profile-b> "<task>"
skycode profile bench <profile>
skycode profile tune <model-name>
skycode profile export-results
```

- `list`: enumerates profiles defined in registry.
- `show`: prints resolved settings (after `auto`/`tuned` resolution).
- `use`: persists the active profile selection in `~/.skycode/state/active_profile`. Subsequent commands honor it; can be overridden per-task via `--profile`.
- `test`: runs the task once under the named profile against the active model and writes a `tuning_runs` row.
- `compare`: runs the same task under each profile, writes two `tuning_runs` rows with shared `prompt_hash`, prints a diff of metrics + a short text diff of outputs.
- `bench`: runs a fixed synthetic prompt set under the profile (decode 4096 tokens, prompt eval, ctx fill) and records metrics.
- `tune`: runs the §5.4 tuning loop and writes the result back to the registry as a new derived profile. Always safety-checked against the forbidden field list.
- `export-results`: serializes `tuning_runs` to a JSONL/CSV file for offline analysis.

### B.3 Testing Lab result store

`tuning_runs` table is defined in §4.4 under "Tuning lab".

Each row records: model, profile, task class (when applicable), `prompt_hash` (so identical prompts compare cleanly), the resolved `settings_json`, `result_summary` (truncated, secret-redacted), first-token latency, decode tok/s, prompt-eval tok/s, peak VRAM, peak RAM, ctx requested vs achieved, output tokens, error code, timestamp.

Rows are also reflected as `tuning_run_started`/`tuning_run_completed` events in `tool_events` for unified audit replay.

### B.4 Safety constraints

- Profiles tune model behavior; they cannot weaken policy.
- Profiles cannot enable remote adapters (loader rejects `enabled`, `base_url`, `api_key_env`).
- Profiles cannot bypass approval, widen tool permissions, disable secret scanning, or disable audit.
- Profiles cannot enable silent writes.
- Invalid profile settings fail clearly with file:line and a non-zero exit (code 8).
- All profile changes are visible in `skycode doctor` (resolved active profile, source).
- Effective profile is logged with every `model_invoked` event (`profile_name`).

### B.5 Testing Lab goals

`profile compare` and `profile bench` produce comparable measurements on:

- speed (wall-clock to first token; total decode time)
- first-token latency
- decode tokens per second
- prompt-evaluation speed
- peak VRAM
- peak RAM
- context achieved vs requested
- output quality notes (`result_summary`; user can free-text annotate)
- failures (`error_code`)
- OOM events
- policy denials
- rollback need (when test runs a code-edit task class)
- diff validity (parse + apply dry-run)

### B.6 Acceptance criteria

```
profile::list_profiles_from_registry
profile::use_profile_updates_effective_runtime_config
profile::profile_cannot_enable_remote
profile::profile_cannot_weaken_policy
profile::profile_test_logs_tuning_run
profile::profile_compare_records_two_runs
profile::doctor_shows_effective_profile
profile::model_invoked_event_contains_profile_name
profile::invalid_profile_setting_fails_clear
profile::tune_writes_best_safe_profile_to_registry
```

### B.7 Documentation deliverables

Add to §4.15:

```
docs/runtime-tuning.md
docs/testing-lab.md
```

### B.8 V1 success-flow integration

The V1 success scenario in §11 includes, before `skycode ask`:

```
skycode profile list
skycode profile bench precise
skycode profile use precise
```

`tuning_runs` will hold at least one row for `(local-coder, precise)` after a successful run. `tool_events` will contain `tuning_run_started` and `tuning_run_completed` events.

### B.9 V1 scope justification

Testing Lab is permitted in V1 because it tunes local runtime behavior only. It does not introduce multi-agent behavior, UI, remote-default behavior, embeddings, voice, or autonomous execution. Its safety surface is fixed by the loader's forbidden-field allowlist (B.1) and by the unchanged orchestrator/agent/tool/secret/trust contracts.

---

## Summary of Changes

This master plan integrates the following on top of `docs/PLAN.md`:

- **Boundary correction (P1):** SkyCore client lives in the orchestrator, not the agent. Agent runtime now has zero handles (filesystem, network, process, model, tools, write APIs). Phase 4 pipeline rewritten accordingly. Crate-level deps and lints enforce.
- **Immutable diff storage (P2):** `diff_proposals` table; `approve` operates on stored rows; `apply` resolves diff bytes from storage and verifies base blob hashes.
- **Event sourcing (P3):** `tool_events` carries `event_type` (closed enum) instead of a mutable `status`; state of any task is derived by replay; triggers block UPDATE/DELETE.
- **Replay defense (P4):** dedicated `approval_tokens_used` table with PK uniqueness as the single-use guarantee, decoupled from log id collision.
- **Realistic tiered hardware (P5):** required-minimum / target / fallback tiers replace single hard gate; fallback path defined; mlock/no-mmap remain recommended on fallback.
- **Sandbox (P6):** explicit subprocess policy; allowlist; categorical blocks even with policy override; environment scrub; no shell interpolation.
- **Secrets (P7):** `skycode-policy` crate; filename + content patterns; `.gitignore` honored; pre-prompt + pre-log redaction; remote-adapter hard-block on secret-origin context; CLI+policy double-confirmation override.
- **Configuration hierarchy (P8):** global / project / policy / agent layout; restrict-only `policy.yaml`; safe-settings-only `project.yaml`; schema-versioned; hot-reload rules.
- **Project trust (P9):** `skycode trust*` CLI; trust record schema; untrusted vs trusted modes; remote-stale invalidation; policy-widening re-trust.
- **Pinning (P10):** `docs/PINS.yaml`; `skycode doctor`, `version`, `model verify <name>`; runtime-flag probes refuse unverified claims.
- **Git isolation (P11):** isolated `skycode/apply/<task-id>` branches; never moves user HEAD by default; `applied_changes` immutable; rollback status derived from events; dirty-tree rejected by default.
- **Context budget (P12):** explicit per-slot budget; per-request context report; ≥50% reduction made measurable.
- **Diagnostics (P13):** `doctor`, `logs`, `audit`, `context`, `graph stats`, `memory stats`; all read-only; never mutate audit tables.
- **Migrations & backup (P14):** versioned, idempotent, transactional, sha256-tamper-detected migrations; `backup` / `restore` CLI; signing-key excluded by default.
- **Documentation deliverables (P15):** canonical `/docs/*.md` set extended; precedence rules formalized.
- **Acceptance criteria refresh (P16):** §11 success flow rewritten to exercise every patch; release-blocker tests enumerated.
- **Runtime tuning profiles & testing lab (P21):** `profiles:` block in registry; `skycode profile *` CLI; `tuning_runs` table; closed allowlist of tunable fields; explicit forbidden-field set; `model_invoked` events carry `profile_name`; tuning-lab goals and acceptance criteria.

The plan keeps every V1 non-negotiable: single agent, CLI-only, local-first, no Tauri, no reviewer, no multi-agent, no embeddings, no voice, no multimodal, no personality drift, no emotional valence, no active relationship memory, no remote-default, no swarm/civilization framing inside V1.

## Assumptions and Unresolved Conflicts

**Assumptions:**

- Default tree-sitter language commitment is Python + TypeScript + Rust based on the user's stated stack; other languages auto-detect with degraded (file/folder-only) graph data.
- `llama-server` (subprocess) is the V1 default integration vehicle for llama.cpp; FFI is an upgrade path. This decision is recorded under Phase 0 and is reversible without protocol changes.
- Tuning profile field set (`temperature`, `top_p`, `top_k`, `repeat_penalty`, mirostat, presence/frequency penalties, KV cache types, `n_cpu_moe`, `ctx_size`, `speculative`) covers the universe of `llama-server` knobs needed for the named profiles. Additional knobs require an explicit registry schema bump and a new patch.
- `docs/PINS.yaml` is a checked-in YAML at repository root under `docs/`. Any other location requires a small adjustment to `skycode doctor` / `skycode version`.
- Default approval-token TTL of 300 seconds is sufficient for interactive CLI workflows; long-pause approvals require a fresh `skycode approve <diff-id>`.

**Unresolved conflicts:**

- `/docs/memory-system.md` lists `relationships` as a V1 scope ("Relationship memory (lightweight)"). This plan keeps the table created in V1 but **inactive** (no V1 code path writes to it). If a strict reading of `/docs` requires writes, this plan's position is that lightweight *storage* is V1 (table exists) and lightweight *use* is post-V1 (no active mechanism); reconciliation belongs in Phase 0.
- `/docs/roadmap.md` Phase E (15–24 weeks) lists "reviewer and manager roles" and "Tauri UX" with weeks that overlap a 24-week reading of V1. This plan treats Phase E as v1.5 / post-V1 (consistent with ChatGPTV2 and ClaudeV2). If `/docs` is read as making Phase E part of V1 acceptance, the reviewer-agent non-negotiable in this plan conflicts; reconciliation belongs in Phase 0.
- `/docs/protocol.md` shows `model_policy.fallback: "remote-strong"` in the example. This plan defaults fallback to `local-fallback` per non-negotiable 12. The example in `/docs/protocol.md` should be updated to `local-fallback` during Phase 0 to align — or left as a non-default illustrative value with a note. Pending decision.
- `/docs/tool-system.md` lists `apply_diff` and `rollback` without specifying isolation semantics. This plan adds §4.12 git isolation; if `/docs/tool-system.md` is later expanded with a different isolation model, this plan's §4.12 yields.

These three conflicts are noted so Phase 0's "freeze" issue can resolve them explicitly before any code lands.
