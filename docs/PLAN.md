# Skycode V1 — Canonical Implementation Plan

This document supersedes ChatgptPlan, ChatgptPlanV2, GithubCopilotPlan, GeminiPlan, GeminiPlanV2, ClaudePlan, ClaudePlanV2, DeepSeekPlan, DeepSeekPlanV2. `/docs/*.md` remain canonical above this plan; where this plan and `/docs` disagree, `/docs` wins. All prior `*Plan*.md` files in `/docs/Plans/` are now historical input.

---

## 1. Executive Summary

Skycode V1 is one persistent local coder agent (`coder-primary`) sitting behind a strict layered runtime: `Models → Inference Runtime → SkyCore Protocol → Agent Runtime → Memory + Graph + Tools → Orchestrator → CLI`. The agent proposes unified diffs, never writes silently, retrieves context from a SQLite/FTS5 memory store and a tree-sitter structural graph, and runs entirely offline against a local llama.cpp GGUF model. CLI is the only V1 interface. Reviewer agent, manager/architect, Tauri UI, vector embeddings, multi-agent orchestration, remote fallback enabled by default, voice, multimodal, swarm, personality drift, emotional valence, trust scoring, and relationship memory as an active mechanism are post-V1. Not negotiable.

V1 is successful when a developer runs `skycode ask "<refactor>"` on a 50k-line repository with the network disabled, gets a graph-derived diff proposal, approves it via a signed single-use token, and the agent recalls the decision three sessions and two restarts later — with zero unapproved writes in the audit log and a measurable ≥50% context-token reduction versus naive file-dump retrieval.

V1 must be usable on a worst-case 2018-class workstation: 6GB VRAM (GTX 1060 class), 4-core no-AVX-512 CPU (i3-8100 class), 24GB DDR4, PCIe Gen 3, SATA SSD, offline. Better hardware is upside, not requirement. Phase 5 hardening must include a benchmark on this exact class.

---

## 2. Non-Negotiables

1. **No silent writes.** File mutation only via `apply_diff(approval_token)`. Any other write path is a release blocker.
2. **Layer separation strictly enforced.** No layer skips a successor. CLI never sees provider formats; Agent Runtime never calls Inference Runtime directly.
3. **Local-first.** Every V1 exit gate must pass with the network disabled.
4. **No full-repo context dumps.** Context is built from `memory:*` and `graph:*` refs, not file contents enumerated in bulk.
5. **Agent never bypasses orchestrator.** Tool execution and model invocation live in the orchestrator. Agent Runtime returns a `diff_proposal` JSON; it has no filesystem handle and no model handle.
6. **SQLite + FTS5 only for memory.** No vector DB, no embeddings until FTS5 measurably fails a retrieval benchmark.
7. **Single agent before multi-agent.** Exactly one agent (`coder-primary`) exists in the V1 runtime.
8. **All-Rust runtime past Phase 3.** No Python in the runtime path. Agent definitions are YAML (declarative, not code).
9. **Append-only tool log.** `tool_events.id = sha256(payload)`. No `UPDATE`/`DELETE` from the application layer.
10. **Signed single-use approval token.** UUIDv4, TTL=300s, scope-bound, diff-id-bound, validated in the policy layer before `apply_diff`.
11. **Provider format never crosses SkyCore.** Architectural test must enforce.
12. **Remote adapter disabled by default.** Optional, isolated behind Inference Runtime, must not affect any V1 gate when disabled.
13. **Reviewer agent is post-V1.** Not a Phase 5 deliverable in any framing.
14. **Speculative decoding off by default.** Documented reason in §5.3.
15. **Model registry is data, not code.** Hot-reloadable YAML. No model name hardcoded in Rust.
16. **Worst-case hardware is the V1 target.** Phase 5 benchmark must run on the §HARDWARE TARGET class.

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
+-----------------------------------------------------------------+
        |                     |                     |
        | task                | context             | tool calls
        v                     v                     v
+----------------+   +-------------------+   +---------------+
|  Agent Runtime |   | Memory + Graph    |   |    Tools      |
|  (no fs/model  |   | (SQLite + FTS5 +  |   | (read/diff/   |
|   handles)     |   |  tree-sitter)     |   |  apply/git)   |
+----------------+   +-------------------+   +---------------+
        |                                            |
        | SkyCore request                            | filesystem/git
        v                                            v
+-----------------------------------------------------------------+
|                       SkyCore Protocol                          |
|         (provider-agnostic JSON; the only model contract)       |
+-----------------------------------------------------------------+
                              |
                              v
+-----------------------------------------------------------------+
|                       Inference Runtime                         |
|     llama.cpp loader | streaming | model registry | adapter*    |
+-----------------------------------------------------------------+
                              |
                              v
+-----------------------------------------------------------------+
|                            Models                               |
|              (GGUF on disk; remote adapter optional)            |
+-----------------------------------------------------------------+

* router and remote adapter are present but constrained:
  - router lands in Phase 5, additive only
  - remote adapter exists as code path but is disabled by default
```

**Allowed boundary crossings** (the *only* permitted edges):

- `CLI → Orchestrator`: user commands.
- `Orchestrator → Memory/Graph`: read/write index, context retrieval.
- `Orchestrator → Tools`: tool execution under policy.
- `Orchestrator → Agent Runtime`: task dispatch, response collection.
- `Agent Runtime → SkyCore Protocol`: structured request emission.
- `SkyCore → Inference Runtime`: provider-neutral model calls.
- `Inference Runtime → Models`: GGUF load + token streaming, or remote adapter when explicitly enabled.
- `Tools → filesystem/git`: read freely; write only inside `apply_diff(token)` after validation.
- `Graph scanner → filesystem`: read-only; writes only to SQLite index.

**Forbidden crossings** (each paired with its preventative control):

- `CLI → Inference Runtime` direct → CLI binary crate has no dependency on the inference crate; verified by `cargo deny`/architectural test.
- `Agent Runtime → Inference Runtime` direct → Agent Runtime crate has no dependency on the inference crate; agent receives only a SkyCore client trait whose impl is owned by the orchestrator.
- `Agent Runtime → Tools` direct → Agent Runtime crate has no dependency on the tools crate; tool calls are *requests* in the SkyCore response that orchestrator executes.
- `Agent Runtime → filesystem` → Agent Runtime crate has no `std::fs` calls; lint check in CI.
- `Tools → write outside apply_diff` → write functions in the tools crate are private; only `apply_diff(token)` is public; token validation is the first statement.
- `Inference Runtime provider format → above SkyCore` → integration test inspects every value reaching Agent Runtime / Orchestrator / CLI for provider-shaped fields (e.g. `choices`, `delta`, `message.role` raw); fails the build if found.
- `Memory → CLI` direct → CLI crate has no dependency on the memory crate; queries route through orchestrator.
- `External user input → SQL` → all queries use parameterized statements; raw string interpolation banned by lint rule.
- `Tool log mutation` → `tool_events` is `STRICT`, content-addressed by `sha256(payload)`; the tool log writer crate exposes no update/delete API; trigger blocks `UPDATE`/`DELETE` at the SQLite level.
- `Remote adapter on by default` → registry default profile has `runtime: local`; CI test asserts that with the default config no socket is opened.

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
    "read_file",
    "list_dir",
    "search_project",
    "git_status",
    "git_diff",
    "create_diff"
  ],
  "model_policy": {
    "preferred": "local-coder",
    "fallback": "local-fallback"
  },
  "output_contract": "diff_proposal",
  "constraints": {
    "max_output_tokens": 4096,
    "stream": true,
    "stop": []
  }
}
```

`output_contract` ∈ `{ "diff_proposal", "answer", "plan" }`.

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

`status` ∈ `{ "ok", "error", "needs_approval", "needs_tool" }`. `error` is `null` or `{ "code": "string", "message": "string" }`. No provider field — `choices`, `delta`, `usage`, raw `role` strings — is permitted at this layer.

### 4.3 Approval token contract

```
token := base64url( payload || "." || ed25519_sig(payload) )
payload := {
  "tid":  "<uuid-v4>",          // token id, single-use
  "scp":  "apply_diff",         // scope; only this scope grants writes
  "did":  "<diff-id>",          // diff_id binding; mismatch rejects
  "tsk":  "<task-id>",          // origin task
  "exp":  <unix-seconds>,       // now + 300
  "kid":  "<key-id>"            // signing key id, rotatable
}
```

Validation steps, in order. Fail fast on first failure:

1. Decode base64url, split on last `.`. Reject on malformed input.
2. Look up signing key by `kid` in keystore. Reject on unknown key.
3. Verify ed25519 signature over payload bytes. Reject on bad signature.
4. Reject if `payload.scp != "apply_diff"`.
5. Reject if `payload.exp <= now()`.
6. Reject if `payload.did` does not match the `diff_id` argument to `apply_diff`.
7. Atomic check-and-set in `tool_events`: insert a row with `status='applied'` and `id = sha256(payload || diff_id || now())`. If a previous row exists for `tid` with status ∈ `{ applied, rejected }`, reject (replay).
8. Only after all checks pass, hand off to the patch applier.

The signing key lives in the user's data dir, generated on first run. Tokens never leave the local process. Key rotation: generate new `kid`, leave old `kid` valid until the longest outstanding token expires (≤300s).

### 4.4 SQLite DDL

```sql
PRAGMA journal_mode = WAL;
PRAGMA foreign_keys = ON;

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
  content,
  tags,
  content='memories',
  content_rowid='rowid',
  tokenize='porter unicode61'
);
CREATE TRIGGER memories_ai AFTER INSERT ON memories BEGIN
  INSERT INTO memories_fts(rowid, content, tags) VALUES (new.rowid, new.content, new.tags);
END;
CREATE TRIGGER memories_ad AFTER DELETE ON memories BEGIN
  INSERT INTO memories_fts(memories_fts, rowid, content, tags) VALUES('delete', old.rowid, old.content, old.tags);
END;
CREATE TRIGGER memories_au AFTER UPDATE ON memories BEGIN
  INSERT INTO memories_fts(memories_fts, rowid, content, tags) VALUES('delete', old.rowid, old.content, old.tags);
  INSERT INTO memories_fts(rowid, content, tags) VALUES (new.rowid, new.content, new.tags);
END;

CREATE TABLE decisions (
  id            TEXT PRIMARY KEY,
  project_id    TEXT NOT NULL,
  agent_id      TEXT NOT NULL,
  task_id       TEXT NOT NULL,
  summary       TEXT NOT NULL,
  rationale     TEXT,
  context_refs  TEXT,                                -- JSON array of refs
  outcome       TEXT NOT NULL CHECK (outcome IN ('approved','rejected','rolled_back')),
  created_at    INTEGER NOT NULL
) STRICT;
CREATE INDEX idx_decisions_task ON decisions(task_id);
CREATE INDEX idx_decisions_proj_agent ON decisions(project_id, agent_id);

CREATE TABLE agent_state (
  agent_id     TEXT NOT NULL,
  project_id   TEXT NOT NULL,
  state_json   TEXT NOT NULL,
  session_id   TEXT,
  updated_at   INTEGER NOT NULL,
  PRIMARY KEY (agent_id, project_id)
) STRICT;

CREATE TABLE tool_events (
  id              TEXT PRIMARY KEY,                  -- sha256(payload)
  task_id         TEXT NOT NULL,
  agent_id        TEXT NOT NULL,
  tool_name       TEXT NOT NULL,
  inputs_hash     TEXT NOT NULL,
  inputs_json     TEXT NOT NULL,                     -- canonical JSON, retained for audit
  output_hash     TEXT,
  approval_token  TEXT,                              -- nullable for read-only tools
  status          TEXT NOT NULL CHECK (status IN ('requested','approved','applied','rejected','rolled_back')),
  created_at      INTEGER NOT NULL
) STRICT;
CREATE INDEX idx_tool_events_task ON tool_events(task_id, created_at);
CREATE INDEX idx_tool_events_status ON tool_events(status);
CREATE TRIGGER tool_events_no_update BEFORE UPDATE ON tool_events BEGIN
  SELECT RAISE(ABORT, 'tool_events is append-only');
END;
CREATE TRIGGER tool_events_no_delete BEFORE DELETE ON tool_events BEGIN
  SELECT RAISE(ABORT, 'tool_events is append-only');
END;

CREATE TABLE graph_nodes (
  id            TEXT PRIMARY KEY,                    -- sha256(project_id || kind || path || name || span)
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
  id            TEXT PRIMARY KEY,                    -- sha256(from_id || to_id || kind)
  project_id    TEXT NOT NULL,
  from_id       TEXT NOT NULL REFERENCES graph_nodes(id) ON DELETE CASCADE,
  to_id         TEXT NOT NULL REFERENCES graph_nodes(id) ON DELETE CASCADE,
  kind          TEXT NOT NULL CHECK (kind IN ('contains','imports','exports','depends_on','tested_by','calls')),
  metadata_json TEXT
) STRICT;
CREATE INDEX idx_edges_from ON graph_edges(from_id);
CREATE INDEX idx_edges_to ON graph_edges(to_id);
CREATE INDEX idx_edges_kind ON graph_edges(kind);

-- Reserved for V2 multi-agent. Created in V1 migration but not written by any V1 code path.
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
recency_decay(dt) = exp(-dt / TAU)   -- TAU = 14d default, configurable per scope
scope_match: 1.0 if matches, 0.5 if compatible, 0.0 if mismatched
```

Impact query:

```sql
WITH RECURSIVE deps(id, depth) AS (
  SELECT from_id, 1 FROM graph_edges
   WHERE to_id = :target_id AND kind IN ('imports','calls','depends_on')
  UNION
  SELECT e.from_id, d.depth + 1
    FROM graph_edges e
    JOIN deps d ON e.to_id = d.id
   WHERE d.depth < :max_depth
)
SELECT n.* FROM graph_nodes n WHERE n.id IN (SELECT id FROM deps);
```

### 4.5 Agent identity YAMLs

```yaml
# agents/coder-primary/core/soul.yaml
id: coder-primary
name: Coder Primary
role: persistent_coder
core_values: [correctness, safety, locality, traceability]
```

```yaml
# agents/coder-primary/core/heart.yaml
communication_style: concise
collaboration_style: solo
error_handling: fail_visible
```

```yaml
# agents/coder-primary/core/mind.yaml
planning_depth: shallow_task_level
risk_tolerance: low
validation_style: pessimist
```

```yaml
# agents/coder-primary/core/doctrine.yaml
must_never:
  - write_without_approval
  - bypass_orchestrator
  - exceed_tools_allowed
  - force_push_history
must_always:
  - produce_diff_before_apply
  - log_tool_events
  - attach_task_id_to_memory_writes
approval_required_for:
  - file_write
  - file_delete
  - patch_apply
priorities:
  1: data_integrity
  2: user_safety
  3: correctness
  4: performance
```

No `mood`, `valence`, `trust`, `voice`, `personality_drift`, `relationship` fields. Adding any such field is a `/docs/agent-definition.md` violation.

### 4.6 CLI surface (V1)

```
skycode scan <project>                  # build/refresh memory + graph index for a project
skycode ask "<task>"                    # propose a change for a task; never applies
skycode diff <task-id>                  # print the unified diff produced for a task
skycode approve <diff-id>               # emits a signed approval token to stdout
skycode apply <approval-token>          # applies a diff; rejects on invalid/expired/replayed token
skycode rollback <change-id>            # reverts a previously applied change
skycode memory search "<query>"         # FTS5 query, scoped to project/agent
skycode graph impact <path-or-symbol>   # recursive dependency query
skycode model load <name>               # ensure a registry entry's model is loadable
skycode model bench <name>              # tok/s, VRAM, RAM, ctx, mmap, mlock, OOM
```

Exit codes: `0` ok, `2` validation failure (bad token, expired, scope mismatch), `3` policy denial, `4` patch conflict, `5` model load failure, `6` integrity failure (log/index inconsistency).

---

## 5. Inference Runtime — Low-VRAM MoE Configuration

This section is normative. The runtime *must* work on the §HARDWARE TARGET class. A naive offload configuration regresses to ~3 tok/s on this hardware and is forbidden as a default.

### 5.1 Default model class: 35B MoE (Qwen3-class, A3B activation)

V1 defaults to a Qwen3-class 35B MoE model with A3B-style activation (a small set of experts active per token, rest dormant). MoE is preferred over a dense 7–13B model for this VRAM budget because the *active* parameter count per token is much smaller than the total weight set: only the always-active path and the currently-routed experts must be in fast memory. With expert blocks pinned to system RAM, peak GPU VRAM is dominated by the active path + KV cache, which fits a 6GB card; the rest streams from the much larger DDR4 budget. A dense model of equivalent quality would not fit. (Adopted: ChatGPTV2/DeepSeekV2 model-class direction — extended here with the explicit MoE rationale.)

### 5.2 Forbidden default: naive `-ngl` half-and-half split

Splitting layers between GPU and CPU with `-ngl <N>` for a MoE model where some attention/MoE blocks land on GPU and the rest on CPU forces every token to cross PCIe for the CPU-side blocks, including expert weights that may need to be paged on demand. On PCIe Gen 3 against a SATA-backed page cache this thrashes and observed throughput collapses to ~3 tok/s. Naive `-ngl` on a MoE model is a release blocker as a default.

### 5.3 Required default flags for low-VRAM target

```
--n-cpu-moe <N>          # pin expert blocks to system RAM. Tune downward from a high
                         # starting value (e.g. 41) until VRAM utilization is ~95%.
                         # Always-active path stays on GPU; only experts cross PCIe,
                         # and only when their batch lights up.
--no-mmap                # prevent the OS from paging expert weights from disk under
                         # memory pressure. Without this, large MoE expert sets are
                         # demand-paged from SSD during inference, regressing tok/s
                         # to disk-bound numbers.
--mlock                  # lock weights resident. Required for long-running stability
                         # under desktop memory pressure. Container caveats:
                         #   LXC: container config must permit mlock (no
                         #        cap_drop=IPC_LOCK; raise memlock rlimit).
                         #   Docker/Podman: --cap-add IPC_LOCK and
                         #        --ulimit memlock=-1:-1.
                         # Verify mlock is actually active post-launch (see test
                         # plan §8); the flag is silently ignored when the kernel
                         # rejects the request.
--cache-type-k q8_0      # KV cache K quantization, baseline.
--cache-type-v q8_0      # KV cache V quantization, baseline.
                         # For >128K context, switch to asymmetric K=q4_0 / V=q3_*
                         # turbo *only if the build accepts the flags*; verify
                         # against `llama-server --help` for the exact build, since
                         # cache-quant flag names drift across llama.cpp releases.
                         # Treat any silent fallback to f16 as a hard error.
--ctx-size 131072        # 128K target window for §11 success criterion.
# speculative decoding: OFF by default.
# Rationale: on MoE with SSM-adjacent layers, draft/verify model thrashes the
# expert cache and observed throughput regressed to ~11 tok/s in our harness.
# Re-enable per-model only after a measured win on the §HARDWARE TARGET class.
```

Builds where `--cache-type-k`/`--cache-type-v` are absent must be detected at startup; the runtime must refuse to claim a quantized cache it could not actually configure. (DeepSeekV2 raised flag-drift; promoted here to a hard check.)

### 5.4 Tuning loop

```
1. load model with default flags from registry entry
2. measure: VRAM, RAM, tok/s, ctx achieved, mmap status, mlock status
3. if VRAM utilization < 90%: decrement --n-cpu-moe by 1, restart, goto 2
4. if VRAM utilization > 96%: increment --n-cpu-moe by 1, restart, goto 2
5. expand context via cache-type quantization until target ctx_size reached
6. if OOM at target ctx_size: increment --n-cpu-moe by 1, restart, goto 5
7. write tuned values back to the registry entry
```

The bench command (§5.7) implements this loop; the result is written to the registry, not memorised by humans.

### 5.5 Model registry YAML (hot-reloadable)

Registry path: `~/.skycode/models.yaml` (overridable via `SKYCODE_MODELS_PATH`). Hot-reloaded on file change; no model name is hardcoded in Rust.

```yaml
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

  - name: remote-strong
    runtime: openai_compatible
    enabled: false                     # disabled by default — see non-negotiable 12
    base_url: ""
    api_key_env: ""
    hardware_class: any
```

### 5.6 Hardware classes (registry tags)

Each registry entry declares one of:

- `low_vram_6gb`: GTX 1060 / RTX 2060 mobile / 6GB RTX 30-series. Defaults: `--n-cpu-moe` high, KV cache q8 baseline, ctx 128K via cache-q.
- `mid_vram_12gb`: RTX 3060 12GB / 4070 mobile. Defaults: `--n-cpu-moe` lower, KV cache q8, ctx 128K f16 viable.
- `high_vram_24gb`: RTX 3090 / 4090. Defaults: `--n-cpu-moe 0`, KV cache f16, ctx 128K+ trivial.
- `cpu_only`: no compatible GPU. Defaults: all layers on CPU, smaller fallback model preferred, ctx 32K.

Selecting the wrong class is a `skycode model load` warning, not a silent fallback.

### 5.7 Bench command

```
skycode model bench <name>
```

Reports, per registry entry:

- tok/s prompt eval (first-token latency)
- tok/s decode (steady state)
- peak VRAM during a 4096-token decode
- peak resident RAM
- achieved ctx (loaded vs requested)
- mmap status (yes/no — must be `no` when `no_mmap: true`)
- mlock status (resident bytes vs total weights — verified, not trusted)
- OOM events during run
- spec-decode status (must be `off` by default)

Required for Phase 5 exit on the §HARDWARE TARGET class.

---

## 6. Phased Execution (12 weeks)

### Phase 0 — Canonical Freeze (Week 0–1)

**Goal.** Lock `/docs` as source of truth. Resolve any contradictions between `architecture.md`, `protocol.md`, `agent-definition.md`, `memory-system.md`, `tool-system.md`, `model-runtime.md`, `roadmap.md`. Initialize Rust workspace; no product behavior yet.

**Deliverables.**
- This document committed; supersedes all `*Plan*.md` predecessors.
- Rust workspace skeleton:
  ```
  skycode/
    crates/
      skycode-core/        # SkyCore types, approval token, errors
      skycode-protocol/    # request/response (de)serialization
      skycode-memory/      # SQLite migrations, FTS5, repo
      skycode-graph/       # tree-sitter parsers, graph DDL, queries
      skycode-tools/       # read/list/search/git/diff/apply/rollback
      skycode-inference/   # llama.cpp ffi + remote adapter trait
      skycode-runtime/     # agent runtime, no fs/model deps
      skycode-orchestrator/
      skycode-cli/
    docs/
    agents/coder-primary/...
  ```
- `Cargo.toml` deny rules for forbidden cross-crate dependencies (cli↛inference, runtime↛fs/model/tools).
- SQLite migration `0001_init.sql` matching §4.4.
- SkyCore Rust types matching §4.1, §4.2 with `#[serde(deny_unknown_fields)]`.
- Approval token sign/verify code, signing key bootstrapped on first run.

**Boundary crossings.** None at runtime — design only.

**Risk / control.** Risk: contradictions in `/docs` propagate to code. Control: a `Phase 0 freeze` issue lists every contradiction and its resolution; closed before Phase 1 starts.

**Exit gates.**
- Zero contradictions in `/docs` (or every contradiction has a documented resolution).
- Every V1 feature mapped to exactly one canonical layer.
- Every write-capable operation has a named approval gate in this plan.
- No V1 task in the plan requires the network.
- `cargo test -p skycode-core -p skycode-protocol` passes for round-trip serialization.

### Phase 1 — Safe Tool Spine (Week 1–3)

**Goal.** A safe edit pipeline that works on real repositories with no LLM in the loop.

**Deliverables.**
- Read tools: `read_file`, `list_dir`, `search_project` (ripgrep-class), `git_status`, `git_diff`.
- Write pipeline: `create_diff` (unified-diff producer), `apply_diff(diff_id, approval_token)`, `rollback(change_id)`.
- Approval token implementation per §4.3 (ed25519, TTL=300s, single-use, scope/diff bound).
- Append-only `tool_events` writer + content-addressable id (`sha256` of canonical JSON payload).
- Rollback = pre-apply git ref captured at apply time; revert by `git reset --hard <ref>` on a worktree-managed branch isolated from user HEAD.
- CLI: `diff`, `approve`, `apply`, `rollback`.

**Boundary crossings.** `CLI → Orchestrator → Tools → filesystem/git`. **Risk:** unauthorised file mutation. **Control:** all write functions in `skycode-tools` are private; `apply_diff(diff_id, token)` is the only public write; token validation runs before any I/O; `tool_events` row inserted with `status='requested'` then `status='applied'` in a single transaction with the patch application.

**Exit gates.**
- 50 simulated edit cycles (`propose → approve → apply → rollback`) produce zero unapproved writes per audit query against `tool_events`.
- A red-team test that calls private write functions via reflection / direct path fails to compile or to find a public entry point.
- Multi-file rollback verified on a real git repo with ≥3 files changed.
- Every tool call produces a queryable `tool_events` row.
- `tool_events` `UPDATE`/`DELETE` triggers fire in tests.

### Phase 2 — Memory + Graph V1 (Week 3–5, overlaps Phase 1 from Week 3)

**Goal.** Retrieval substrate: SQLite memory with FTS5 and a structural graph from tree-sitter.

**Deliverables.**
- All migrations from §4.4 applied; FTS5 triggers active.
- Memory write API: scope-tagged inserts with `task_id` attached.
- Memory retrieval API ranked by `bm25 * recency_decay * importance * scope_match`.
- Project scanner using tree-sitter for **Python, TypeScript, Rust** by default (the user's stack); other languages auto-detected on scan and disabled with a warning if no grammar is bundled. Adopted: ClaudeV2 default-language commitment.
- Graph nodes: `file`, `folder`, `symbol`, `import`, `export`. Edges: `contains`, `imports`, `exports`, `depends_on`, `tested_by`, `calls` (where extractable).
- `skycode graph impact <path-or-symbol>` using the recursive CTE in §4.4.
- Incremental rescan keyed off mtime + size; full rescan on `skycode scan --force`.

**Boundary crossings.** `Orchestrator → Memory/Graph`; `Graph scanner → filesystem (read-only)`. **Risk:** OOM on large repos; FTS5 ranking degradation past ~100k rows. **Control:** scanner streams nodes/edges to SQLite in batches; FTS5 uses external-content table to keep storage compact; retrieval has a hard `LIMIT` and a `bm25` cutoff.

**Exit gates.**
- Scan persists across process restart (verified by reading post-restart counts).
- Memory retrieval returns scoped results (project-only and agent-only filters honoured).
- Graph impact correctly identifies affected files for ≥3 real refactors on a sample repo.
- Memory retrieval p95 latency <200ms on 10k rows.
- No vector DB, no embeddings, no remote service used.
- Architectural test: `skycode-memory` and `skycode-graph` have no dependency on `skycode-inference`.

### Phase 3 — Local Inference + SkyCore (Week 5–7)

**Goal.** Local llama.cpp behind SkyCore. Provider format never crosses the boundary. Model registry is hot-reloadable YAML. Low-VRAM MoE config is the default.

**Deliverables.**
- `skycode-inference` crate with:
  - llama.cpp FFI / `llama-server` subprocess wrapper (decision recorded in Phase 0; the subprocess wrapper is the V1 default for build simplicity, ffi as upgrade path).
  - flag set per §5.3 with §5.6 hardware-class defaults.
  - mlock verification post-launch (resident bytes vs declared model size).
  - cache-type flag presence detection per build of `llama-server`.
  - streaming token interface.
- Model registry loader (`models.yaml` per §5.5), file-watcher hot reload.
- `skycode model load` and `skycode model bench` (§5.7).
- SkyCore serializer/deserializer with `deny_unknown_fields`.
- Optional remote adapter (OpenAI-compatible) behind a `runtime: openai_compatible` registry entry, **disabled by default**; output normalized to SkyCore response shape.
- Tuning loop (§5.4) wired into `skycode model bench`.

**Boundary crossings.** `Agent Runtime → SkyCore → Inference Runtime → Models`. **Risk:** provider format bleed past SkyCore; mlock silently failing in containers; cache-quant flag drift across llama.cpp builds. **Control:** integration test inspects every value at the `Agent Runtime` boundary for provider-shaped fields and fails the build on hit; mlock verifier reports actual locked bytes; runtime refuses to claim quant cache types it could not configure.

**Exit gates.**
- Local model completes a SkyCore request offline end-to-end on the §HARDWARE TARGET class.
- Identical SkyCore request shape produces structurally identical SkyCore response shape from local and (when explicitly enabled) remote adapters.
- Missing model → explicit error with code, no fallthrough to remote.
- Registry hot reload works without process restart (verified in test).
- `skycode model bench local-coder` reports tok/s ≥15 and VRAM ≤5.9 GB on the §HARDWARE TARGET class.
- mlock verifier shows ≥90% of declared model size resident.

### Phase 4 — Persistent Coder Agent (Week 7–10)

**Goal.** The V1 product loop end-to-end on a single agent.

**Deliverables.**
- `coder-primary` instantiated from YAMLs in §4.5 (no extra fields).
- Agent state persisted in `agent_state`, reloaded on start.
- Orchestrator pipeline:
  ```
  classify task
    → retrieve context (memory:* + graph:* refs only; never raw file dumps)
    → build SkyCore request
    → invoke Inference Runtime via SkyCore
    → receive SkyCore response
    → if response is diff_proposal: persist diff, mark task awaiting approval
    → on approve: validate token, apply, log decision + tool events
    → on rollback: revert + log
  ```
- Diff application path goes only through Phase 1 `apply_diff`.
- Decision writer: every applied change writes a `decisions` row with `summary`, `rationale`, `context_refs`, `outcome`.
- CLI: `skycode ask "<task>"` proposes; never applies silently.

**Boundary crossings.** All allowed crossings exercised. **Risk:** agent attempting direct tool/model access; agent context bleeding across projects. **Control:** `skycode-runtime` crate has zero dependency on `skycode-tools`, `skycode-inference`, or `std::fs` (lint-checked); tool calls are JSON requests in the SkyCore response that orchestrator executes; memory queries always include `project_id` filter.

**Exit gates.**
- Agent recalls a decision from session 1 in session 3 after two full process restarts (recall by keyword, by `task_id`, and by `decision.id`).
- One real safe edit completed offline on a 50k-line repository: graph-derived context, diff, approval, apply, log, optional rollback.
- All edits route through `diff → approval → apply → log`. Audit query returns 0 unapproved writes.
- Context tokens for the V1 success scenario ≤50% of a naive baseline that loads referenced files in full.
- Exactly one agent exists in the runtime (asserted at startup).

### Phase 5 — Hardening + Lightweight Router (Week 10–12)

**Goal.** Ship V1. Phase 5 is a release-blocker phase; new feature work is restricted to the lightweight router. Reviewer agent does not appear here under any framing.

**Deliverables (hardening).**
- End-to-end regression suite covering every prior gate.
- Failure-mode tests: missing model file; expired token; double-spent token; scope-mismatched token; diff-id-mismatched token; patch conflict on apply after concurrent external edit; process kill mid-task; rollback when git state has changed externally; SQLite locked; FTS5 corruption recovery; registry YAML invalid; mlock denied (container).
- Offline demo: full V1 flow with the network disabled at the OS level.
- Benchmark: graph-aware retrieval reduces context tokens ≥50% vs naive file-dump on representative tasks.
- Hardware-class bench: `skycode model bench local-coder` on the §HARDWARE TARGET class. Required: tok/s ≥15, VRAM ≤5.9GB, ctx 131072, mlock active, mmap off, OOM count 0.
- Operator docs: CLI reference, model-setup walkthrough, registry tuning notes.

**Deliverables (lightweight router, additive).**
- Task classifier: maps `goal` text → task class ∈ `{ classify, short_answer, code_edit, refactor, plan }` via keyword + structural heuristics. No second model.
- Router: task class → registry entry → fallback chain. Fallback: `local-primary → local-fallback → explicit failure`. No silent remote.
- Telemetry to `tool_events`: latency, tokens, model used, fallback fired (yes/no). (Adopted: ClaudeV2 router-as-Phase-5 placement; reviewer explicitly excluded.)

**Boundary crossings.** None new. **Risk:** release with hidden unsafe path; remote adapter accidentally enabled in offline tests. **Control:** release blocked if any gate fails; CI offline-test asserts no socket open; registry-default test asserts `enabled: false` on remote entries.

**Exit gates.**
- Zero unapproved writes in the full suite.
- Offline demo passes end-to-end.
- ≥50% context-token reduction confirmed on a labeled benchmark.
- Tool + decision logs reconstruct every applied change.
- SQLite remains sufficient under measured V1 workload (read p95 <200ms, write p95 <50ms on 100k memory rows).
- Hardware-class bench passes on the §HARDWARE TARGET class.
- Router selects correct class on ≥9/10 hand-labelled samples; fallback fires correctly on simulated primary failure.

---

## 7. Universal Phase Gate Checklist

Applied at every phase close, not only at the named exit. A phase cannot close until every line is `pass`.

| Gate          | Criterion                                                           |
|---------------|---------------------------------------------------------------------|
| Safety        | Zero unapproved writes in full `tool_events` audit for the phase.   |
| Persistence   | All state required by the phase survives a clean process restart.   |
| Traceability  | Every tool event queryable by `task_id` and timestamp.              |
| Boundary      | No layer boundary crossed outside §3 allowed list.                  |
| Quality       | Phase-specific numeric threshold met as listed in §6.               |

---

## 8. Test Plan

Each test is named so it appears in CI output and can be referenced in a PR.

### Unit

- `approval_token::create_then_verify_succeeds`
- `approval_token::expired_rejected`
- `approval_token::single_use_replay_rejected`
- `approval_token::scope_mismatch_rejected`
- `approval_token::diff_id_mismatch_rejected`
- `approval_token::tampered_signature_rejected`
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
- `safe_edit::concurrent_external_edit_yields_patch_conflict`
- `scan_then_graph_impact_then_context_build`
- `cli::ask_produces_diff_no_silent_apply`
- `restart::agent_recalls_decision_session3_two_restarts`
- `fts5::scoping_project_and_agent_isolation`
- `fts5::p95_under_200ms_at_10k_rows`

### Offline

- `offline::full_v1_flow_with_network_disabled` (uses Linux `unshare -n` / Windows `Disable-NetAdapter` test fixture)
- `offline::ci_default_config_opens_no_sockets`

### Regression / architectural

- `arch::cli_has_no_dep_on_inference`
- `arch::cli_has_no_dep_on_memory`
- `arch::runtime_has_no_dep_on_tools`
- `arch::runtime_has_no_dep_on_inference`
- `arch::runtime_has_no_std_fs_calls`
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

### Hardware-class bench

- `bench::low_vram_6gb::tok_s_ge_15_at_ctx_131072`
- `bench::low_vram_6gb::vram_le_5_9_gb`
- `bench::low_vram_6gb::mlock_actually_active` (parses `/proc/<pid>/status` `VmLck` on Linux; `GetProcessWorkingSetSize` + locked-pages probe on Windows)
- `bench::low_vram_6gb::mmap_disabled_actually` (verifies no `MAP_PRIVATE` mapping of GGUF in `/proc/<pid>/maps`)
- `bench::low_vram_6gb::oom_count_zero_during_4096_decode`
- `bench::all_classes::tok_s_no_regression_vs_baseline_5pct`

---

## 9. Known Problems Pre-Identified

For each: symptom → root cause → mitigation built into V1.

- **PCIe thrashing on naive offload.** Symptom: ~3 tok/s on §HARDWARE TARGET. Cause: `-ngl <half>` puts MoE blocks across the PCIe boundary, every token crosses the bus. Mitigation: §5.3 default forbids naive `-ngl`; `--n-cpu-moe` pins experts to RAM; tuning loop (§5.4) targets 95% VRAM utilization.
- **OS paging of experts under memory pressure.** Symptom: tok/s drops to disk-bound (single-digit) under co-tenant load. Cause: mmap'd GGUF expert pages get evicted. Mitigation: `--no-mmap` + `--mlock` defaults; mlock-actually-active test (§8) verifies; container caveats documented (§5.3).
- **KV cache flag drift across llama.cpp builds.** Symptom: cache-type flags silently ignored, cache stays f16, ctx target unreachable. Cause: flag names change across releases. Mitigation: startup probes `llama-server --help` for the exact flag names; runtime refuses to claim a quant cache type it could not configure; logged as explicit error.
- **mlock silently failing in containers.** Symptom: mlock flag accepted but no pages locked; performance degrades under pressure. Cause: kernel rejects without `IPC_LOCK` cap or memlock rlimit. Mitigation: `bench::mlock_actually_active` measures `VmLck` post-launch; bench fails the run if locked bytes < 90% of declared model size.
- **tree-sitter grammar gaps for less common languages.** Symptom: scan logs nodes but emits no symbol/edge data for some files. Cause: no grammar bundled. Mitigation: V1 commits to Python/TypeScript/Rust; other detected languages produce `file`/`folder` nodes and a clear warning; impact queries degrade gracefully to file-level granularity.
- **FTS5 ranking degradation past ~100k rows.** Symptom: relevant memories fall out of top-N, latency creeps. Cause: bm25 alone, no recency/importance signal at index level. Mitigation: ranking is `bm25 * recency_decay * importance * scope_match` at query time; hard `LIMIT`; benchmark in §8; vector embeddings remain post-V1 (gate-locked) only if measured failure persists after this ranking.
- **Approval token replay.** Symptom: same token applied twice. Cause: in-memory single-use check missed across process restart. Mitigation: §4.3 step 7 uses `tool_events` row insert as the atomic check-and-set; replay rejected even after restart.
- **Patch conflict on apply after concurrent edit.** Symptom: `apply_diff` against drifted file. Cause: external edit between `create_diff` and `apply`. Mitigation: diff is captured against a recorded blob hash; apply verifies blob hash before write; mismatch returns exit code 4 with conflict report. (`failmode::patch_conflict_after_external_edit`.)
- **Agent context bleed across projects.** Symptom: project A retrieval returns project B memories. Cause: missing `project_id` filter in retrieval. Mitigation: every memory/graph query takes `project_id` as a required argument in the repo API; type system disallows unscoped queries; integration test (`fts5::scoping_project_and_agent_isolation`).
- **SkyCore version skew between agent and orchestrator.** Symptom: agent emits a request the orchestrator can't parse. Cause: rolling versions independently. Mitigation: `skycore_version` field is required; orchestrator rejects mismatched majors with a clear error; one workspace, one version in V1 (single-binary deploy).
- **Remote adapter accidentally enabled in offline tests.** Symptom: offline test passes only because it actually called out. Cause: registry default flips to `enabled: true`. Mitigation: `offline::ci_default_config_opens_no_sockets` test; registry-default unit test asserts `enabled: false` on every `remote-*` entry.
- **Graph index staleness after external git operations.** Symptom: impact query returns moved/deleted symbols. Cause: no rescan after `git checkout`/`git pull`. Mitigation: scanner watches `.git/HEAD` and `.git/refs`; on change, queues an incremental rescan; `skycode scan --force` available; staleness flag exposed in `skycode graph impact` output.

---

## 10. Post-V1 Backlog (gate-locked)

Each item is locked behind an explicit promotion trigger. None are V1 deliverables in any framing.

- **Reviewer agent (coder proposes, reviewer critiques, human approves).** Trigger: all V1 gates pass *and* 2 weeks of stable V1 use on a real codebase.
- **Remote model fallback enabled by user config.** Trigger: V1 ships and a labeled task-class benchmark shows ≥10% quality lift from remote on `refactor`/`plan` classes, with explicit user opt-in.
- **Manager / architect agents.** Trigger: reviewer agent stable for 4 weeks; multi-agent orchestration design accepted.
- **Tauri UI.** Trigger: reviewer agent in production; CLI usability benchmarks identify a UI-amenable workflow.
- **Vector embeddings.** Trigger: FTS5 + recency + importance + scope ranking measurably fails on a labelled retrieval benchmark — recall@10 < target on representative queries — *after* tuning the existing rank function.
- **Multi-agent orchestration.** Trigger: reviewer + coder pair stable; explicit multi-agent contract added to SkyCore.
- **Relationship memory as active mechanism.** Trigger: multi-agent in production. Until then, the `relationships` table remains dormant.

Never: swarm consensus, personality drift, emotional valence, voice, multimodal, AI-civilization framing.

---

## 11. V1 Success Definition

A developer on a §HARDWARE TARGET workstation, network disabled, runs the following on a 50k-line repository:

```
$ skycode scan .
$ skycode ask "extract auth logic in src/auth/* into a new src/services/auth_service.* module \
                and update call sites"
```

The system:

1. Pulls context from memory and graph only — `graph:symbol:Auth*`, `graph:file:src/auth/*`, prior `decision:*` rows. No file-dump payload to the model.
2. Produces a SkyCore response with `output_contract = diff_proposal` and `requires_approval = true`. CLI prints the unified diff.
3. Developer runs `skycode approve <diff-id>` → receives a signed token (TTL=300s, single-use, scope=`apply_diff`, bound to the diff id).
4. Developer runs `skycode apply <token>` → policy validates per §4.3, patch applies via `apply_diff`, `tool_events` rows for `requested → approved → applied` are written, `decisions` row written with `summary`, `rationale`, `context_refs`, `outcome=approved`.
5. Three sessions later and after two full process restarts, `skycode ask "why did we extract AuthService?"` retrieves the prior `decision` and the agent cites its rationale.

Numeric thresholds, all required:

- Context-token reduction ≥50% vs naive file-dump baseline on this scenario.
- Decode throughput ≥15 tok/s on the §HARDWARE TARGET class with the §5 default flags.
- Zero unapproved writes in the `tool_events` audit for the entire run.
- Achievable context window ≥131072 tokens on the §HARDWARE TARGET class (KV cache quantization per §5.3).
- mlock active: locked bytes ≥90% of declared model size, verified per §8.
- Full flow completes with the network disabled at the OS level.

That is V1. Reviewer, UI, multi-agent, embeddings, remote-default, and everything else in §10 come after.
