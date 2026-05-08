# Skycode DeepSeek Plan v2

## 1. V1 Autopsy (What Was Wrong)

- **Over-scoped vision:** “AI civilization,” “digital ecosystem,” “living software company” are post-v1 language. V1 is one agent, safe tools, memory, graph.
- **Premature multi-agent:** Swarm execution, consensus, reviewer/architect/ops agents, trust scoring, emotional valence, personality drift—all deferred.
- **Wrong sequencing:** Multi-agent before single-agent reliability; vector DB before SQLite sufficiency; voice/UI before core loop.
- **Runtime confusion:** Agent communication outside orchestrator; direct model access; relationship memory as active coordination mechanism for v1.
- **Rhetoric over engineering:** Emotional scores, trust deltas, mood fields, evolution.log. All cut.

## 2. Strongest Ideas Stolen from Other Plans

- **ChatGPTPlan:** Strict scope cuts, “first useful, then intelligent, then social, then alive” ordering, explicit phase exit gates, FTS5-first retrieval, signed approval tokens, append-only tool log, and the SkyCore contract template.
- **ClaudePlan:** Phase 0 freeze, tree-sitter over custom parsing, exact 12-week phasing with hard gates, approval token as signed UUID with TTL, “coder proposes, reviewer critiques” deferred, all-Rust runtime preference, graph retrieval reduction target (≥50% context reduction).
- **GitHubCopilotPlan:** Non-negotiable list, verification gates (safety, persistence, traceability, quality) per phase, immediate implementation checklist.
- **GeminiPlan (v2):** Defended 4-file agent identity (stripped of emotion), graph node/edge vocabulary, explicit boundary crossing/flags for every phase, local-first offline gate, and the strict “no provider bleed” contract.

## 3. Defended v1 Positions (Stripped of Excess)

- **Soul/Heart/Mind/Doctrine split:** Kept, but each contains only essential fields for identity, behavior preferences, reasoning constraints, and hard rules. Emotional traits removed. Heart.yaml now describes communication style and operational behavior only. Mind.yaml limits to planning depth and risk tolerance. Doctrine.yaml is the agent’s policy.
- **Persistent memory across sessions:** Core. V1 uses SQLite with FTS5, keyword+recency+importance retrieval. No vector embeddings.
- **Graph cognition:** Core. V1 structural graph: files, imports, exports, symbols, dependency edges. tree-sitter for parsing. Impact analysis and context reduction.
- **SkyCore protocol:** Core. Every model/provider normalized before agent sees it.
- **Local-first:** Core. llama.cpp GGUF runtime, no remote dependency for v1 gates.

## 4. Layer Boundaries & Breach Risks

Canonical stack:

```
Models → Inference Runtime → SkyCore Protocol → Agent Runtime
→ Memory + Graph + Tools → Orchestrator → CLI
```

Inviolable rules:

- CLI never sees provider formats.
- Agent Runtime never calls models directly.
- Every write goes through policy → diff → approval token (signed UUID, TTL) → apply → log.
- Tool event log is append-only, immutable, content-addressable.
- Memory is SQLite only; FTS5 for retrieval.
- Graph scanner is read-only except writing index to SQLite.

## 5. V1 Core Schemas

### SkyCore Request

```json
{
  "task_id": "uuid",
  "agent_id": "coder-primary",
  "goal": "Refactor auth module",
  "context_refs": ["memory:dec-23", "graph:symbol:AuthService"],
  "tools_allowed": ["search_code", "create_diff"],
  "output_contract": "diff_proposal"
}
```

### SkyCore Response

```json
{
  "task_id": "uuid",
  "status": "ok",
  "summary": "Extracted auth logic to service.",
  "artifacts": ["diff:patch-001"],
  "requires_approval": true
}
```

### Agent Identity (Minimal)

```yaml
# soul.yaml
id: coder-primary
name: Coder Primary
role: persistent_coder
core_values: [correctness, safety, locality]

# heart.yaml
communication_style: concise
collaboration_mode: solo
error_handling: fail_visible

# mind.yaml
planning_depth: shallow_task_level
risk_tolerance: low
validation_style: pessimist

# doctrine.yaml
must_never:
  - write_without_approval
  - bypass_orchestrator
must_always:
  - produce_diff_before_apply
  - log_tool_events
approval_required_for:
  - file_write
  - file_delete
  - patch_apply
```

### SQLite V1 Tables

```sql
CREATE TABLE memories (
  id TEXT PRIMARY KEY,
  project_id TEXT NOT NULL,
  agent_id TEXT NOT NULL,
  scope TEXT CHECK(scope IN ('project','agent','session','decision')) NOT NULL,
  content TEXT NOT NULL,
  importance REAL DEFAULT 0.5,
  created_at TEXT DEFAULT (datetime('now')),
  updated_at TEXT DEFAULT (datetime('now'))
);
CREATE VIRTUAL TABLE memories_fts USING fts5(id UNINDEXED, content, scope UNINDEXED, project_id UNINDEXED, agent_id UNINDEXED);

CREATE TABLE decisions (
  id TEXT PRIMARY KEY,
  project_id TEXT NOT NULL,
  agent_id TEXT NOT NULL,
  summary TEXT NOT NULL,
  rationale TEXT,
  context_refs TEXT, -- JSON array
  created_at TEXT DEFAULT (datetime('now'))
);

CREATE TABLE agent_state (
  agent_id TEXT NOT NULL,
  project_id TEXT NOT NULL,
  state_json TEXT NOT NULL,
  updated_at TEXT DEFAULT (datetime('now')),
  PRIMARY KEY (agent_id, project_id)
);

CREATE TABLE tool_events (
  id TEXT PRIMARY KEY,
  task_id TEXT NOT NULL,
  tool_name TEXT NOT NULL,
  input_hash TEXT NOT NULL,
  output_hash TEXT,
  status TEXT CHECK(status IN ('requested','approved','applied','rejected','rolled_back')) NOT NULL,
  created_at TEXT DEFAULT (datetime('now'))
);

CREATE TABLE graph_nodes (
  id TEXT PRIMARY KEY,
  project_id TEXT NOT NULL,
  kind TEXT CHECK(kind IN ('file','folder','symbol','import','export')) NOT NULL,
  name TEXT NOT NULL,
  path TEXT,
  span_json TEXT,
  metadata_json TEXT
);

CREATE TABLE graph_edges (
  id TEXT PRIMARY KEY,
  project_id TEXT NOT NULL,
  source_id TEXT NOT NULL,
  target_id TEXT NOT NULL,
  kind TEXT CHECK(kind IN ('contains','imports','exports','depends_on','tested_by')) NOT NULL,
  metadata_json TEXT
);

-- Lightweight relationship storage for v1 (used only by future multi-agent)
CREATE TABLE relationships (
  agent_id TEXT NOT NULL,
  target_id TEXT NOT NULL,
  note TEXT,
  created_at TEXT DEFAULT (datetime('now')),
  PRIMARY KEY (agent_id, target_id)
);
```

## 6. Phased Execution Plan

### Phase 0: Canonical Freeze (Week 0–1)

- Align this plan with `/docs`; `/docs` wins.
- Define SkyCore JSON contract (request/response).
- Finalize SQLite migrations and tool contracts.
- Mark all deferred items: multi-agent, UI, voice, vector DB, swarm, personality drift, remote fallback.

**Exit gates:**
- No plan contradiction with `/docs`.
- Every write operation has a named approval gate.
- No v1 task requires network.

**Boundary crossings:** Design only.

### Phase 1: Safe Tool Spine (Week 1–3)

Deliverables:

- `read_file`, `list_dir`, `search_project`, `git_status`
- `create_diff`, `apply_diff`, `rollback`
- Signed approval token: UUID, TTL=5min, single-use, verified before apply.
- Append-only `tool_events` log.
- CLI: `diff`, `approve`, `apply`, `rollback` commands.

**Exit gates:**
- 50 simulated edits produce zero unapproved writes.
- Direct filesystem write paths outside `apply_diff` fail tests.
- Multi-file rollback works on a real git repo.

**Boundary crossings:**
- CLI → Orchestrator.
- Orchestrator → Tools.
- Tools → filesystem/git.
- *Risk:* file mutation. Controlled by diff → approval → apply path only.

### Phase 2: SQLite Memory + Graph V1 (Week 3–5)

Deliverables:

- Migrate all memory tables.
- FTS5 indexing, keyword search ranked by recency * importance * scope.
- Project scanner: walks filesystem, extracts graph nodes/edges with tree-sitter for supported languages (Python, TypeScript, Rust initially).
- Graph impact query: `skycode graph impact <symbol_or_path>` returns directly dependent files.

**Exit gates:**
- Scan persists across restarts.
- Memory retrieval returns scoped results (project + agent).
- Impact query identifies affected files for at least 3 real refactors.
- No vector DB or embeddings used.

**Boundary crossings:**
- Orchestrator → Memory/Graph (read/write index).
- Graph scanner → read-only filesystem.
- *Risk:* none for write; read-only scanner.

### Phase 3: Local Inference Runtime + SkyCore (Week 5–7)

Deliverables:

- llama.cpp GGUF loader, streaming, context config.
- Model registry: name, runtime, context size, strengths, speed.
- SkyCore serializer/deserializer.
- Simple manual model selection (no router yet).
- Optional remote adapter gated behind config flag, disabled by default; must not break offline gates.

**Exit gates:**
- Local model completes a SkyCore request with zero network.
- Provider-specific tokens never reach Agent Runtime or CLI.
- Model load failure yields explicit error.

**Boundary crossings:**
- Agent Runtime → SkyCore Protocol.
- SkyCore → Inference Runtime.
- Inference Runtime → Models.
- *Risk:* remote dependency. Controlled by local-first default, remote disabled in tests.

### Phase 4: Persistent Coder Agent (Week 7–10)

Integrate all previous phases into the V1 loop.

Deliverables:

- Agent `coder-primary` instantiated from yaml.
- Orchestrator pipeline:

```
task classify → context build (memory + graph) → agent invoke
→ SkyCore request → model response → diff proposal
→ approval request → apply approved diff → log
```

- State persistence: agent loads previous state on restart.
- CLI `skycode ask "<task>"` flow.

**Exit gates:**
- Agent recalls a decision from session 1 in session 3 after process restart.
- One real code edit completed offline end-to-end: diff generated, approved, applied, logged, rolled back if needed.
- No unapproved writes occurred during testing.
- Context built from memory/graph refs, not full file dumps.

**Boundary crossings:**
- CLI → Orchestrator.
- Orchestrator → Agent Runtime.
- Orchestrator ↔ Memory/Graph/Tools.
- Agent Runtime → SkyCore.
- *Risk:* agent attempting direct tool access. Prevented because agent runtime has no filesystem permission; it returns a diff proposal JSON, and orchestrator handles tool execution only with approved tokens.

### Phase 5: V1 Hardening (Week 10–12)

Deliverables:

- Regression test suite covering all gates.
- Benchmark context reduction vs naive file dump; must show ≥50% token reduction on representative tasks.
- Failure-mode tests: missing model, expired approval token, patch conflict, restart mid-task.
- Operator docs for CLI and model setup.

**Exit gates:**
- All previous exit gates re-verified.
- Zero unapproved writes in full test suite.
- Offline demo works with network disabled.
- Tool and decision logs reconstruct every applied change.

**Boundary crossings:** All runtime boundaries exercised; no new risks.

## 7. Non-Negotiables (from GitHubCopilotPlan)

1. No silent writes.
2. Layer separation strictly enforced.
3. Local-first operation mandatory for V1 gates.
4. No full-repo context dumps; graph/memory retrieval only.
5. Agent never bypasses orchestrator for tools or model access.
6. SQLite only until retrieval quality measurably fails.
7. Single agent before multi-agent.

## 8. Deferred (Post-V1 Only After All Gates Pass)

- Reviewer agent.
- Manager/architect roles.
- Tauri UI.
- Remote model fallback enabled by user config.
- Vector DB/embeddings.
- Voice, multimodal, swarm, personality drift, relationship scoring.
- Any “civilization” language.

## 9. Immediate Implementation Checklist

1. Confirm `/docs` alignment; update any conflicting statements.
2. Initialize Rust workspace with `runtime`, `cli` crates.
3. Create SQLite migration scripts with schema above.
4. Implement `tool_events` append-only logger.
5. Build diff generator and apply/rollback safety shell.
6. Wire tree-sitter for chosen languages.
7. Integrate llama.cpp and define model registry YAML.
8. Code minimal orchestrator state machine.

This plan supersedes DeepSeekPlan.md and is the single source of truth for V1 implementation.