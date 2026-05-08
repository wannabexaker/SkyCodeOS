# Skycode Implementation Plan — Claude V2

## 1. V1 Autopsy (What Was Wrong)

- **Phase 5 overloaded.** I bundled router + reviewer into weeks 10–12. Reviewer is multi-agent and violates "single agent before multi-agent." It belongs in v1.5/post-v1, not v1. Copilot V1 already corrected this with "reviewer in v1.5"; my V1 didn't.
- **Router timing wrong.** I put router in Phase 5. ChatGPT V2 and Copilot V2 both place it after graph and *not bundled with another deliverable*. Router should be its own phase, after agent + graph are stable, with manual selection sufficing until then.
- **Rust-vs-Python decision was hand-wavy.** I said "all-Rust is cleaner" without committing. Copilot V2 makes it a hard line: all-Rust past Phase 3, no drift. Adopt that.
- **Approval token was specified but not contractually scoped.** "Signed UUID with TTL" — what TTL? Single-use? Validated where? DeepSeek V2 and ChatGPT V2 nailed it: TTL=300s, single-use, validated in policy layer before `apply_diff`. My V1 was vague.
- **Tool log was named but not bounded.** "Append-only and content-addressable" without schema. Copilot V1 spec'd the table; I should have too.
- **No test plan.** ChatGPT V2 has a concrete test inventory. I had gates but no test list. A gate without tests is a wish.
- **No language scope for tree-sitter.** Said "top 3 languages" without committing. Should default to Python + TypeScript + Rust (the user's actual stack) and detect-on-scan for additions.

## 2. Strongest Ideas Stolen from Other V2 Plans

- **ChatGPT V2:** Concrete CLI surface (`skycode scan`, `ask`, `diff`, `approve`, `apply`, `rollback`, `memory search`, `graph impact`). Explicit allowed boundary-crossing list. Explicit test plan with unit + integration + offline + regression categories. Phase 5 as "hardening only" — a release blocker phase, not a feature phase.
- **DeepSeek V2:** Full SQLite schema with CHECK constraints (`scope IN ('project','agent','session','decision')`, `status IN ('requested','approved','applied','rejected','rolled_back')`). FTS5 virtual table mirroring memories. Reserved relationship table for v2 migration without activating it. Adopt the schema verbatim.
- **Copilot V1 (defunct but mined):** "Phase A/B/C/D/E" naming with explicit overlap weeks (B 2-7, C 6-10, D 9-15) — overlap is real because tool spine doesn't block agent dev once write-lock is proven. Adopt overlap windows. Universal phase gate checklist (Safety, Persistence, Traceability, Boundary, Quality). Adopt as a closing checklist for every phase, not just exit criteria.
- **Gemini V2:** "Boundary Crossing" + "Risk" + "Control" annotation per phase. Forces explicit reasoning about which layer boundaries each phase crosses and how the breach is contained. Adopt verbatim.
- **DeepSeek V2 + ChatGPT V2 (both):** Reviewer agent and remote model fallback are *post-v1, gate-locked*. Don't list them as v1 deliverables even with disclaimers.

## 3. Defended V1 Positions

- **12-week timeline.** ChatGPT V2 keeps 12, DeepSeek V2 keeps 12, Copilot V1 had 12-16, Gemini V2 implies ~12. Convergence is honest. 8 weeks is wishful, 24 weeks includes v2 scope.
- **Phase 0 freeze before any code.** All V2s now agree. This was right.
- **SQLite FTS5 first, no embeddings.** All V2s agree. Hold the line.
- **tree-sitter, not custom parsing.** All V2s agree.
- **All-Rust past Phase 3.** Copilot V1 stated this; I'm now committing. Python tempts LangChain-flavored mush. Agent definitions stay as YAML (declarative, language-neutral) — they aren't "Python code."
- **`soul/heart/mind/doctrine` minimal split.** All V2s agree, all stripped of emotion. Hold.
- **Reviewer is post-v1.** Copilot V1, ChatGPT V2, DeepSeek V2 all agree now. My V1 was wrong to include it.

## 4. Layer Boundaries (frozen)

```
Models → Inference Runtime → SkyCore Protocol → Agent Runtime
       → Memory + Graph + Tools → Orchestrator → CLI
```

Inviolable:

- CLI never sees provider formats.
- Agent Runtime never calls Inference Runtime directly; only via SkyCore.
- Every write: policy → diff → signed approval token (TTL=300s, single-use) → apply → append-only log.
- Tool event log is append-only, content-addressable (sha256 of payload as id), no UPDATE/DELETE at app layer.
- Memory is SQLite + FTS5 only.
- Graph scanner is read-only against filesystem; writes only to SQLite index.
- No remote model access required for any v1 exit gate.

## 5. V1 Schemas (canonical, copy DeepSeek V2 with refinements)

### SkyCore Request / Response

```json
// Request
{
  "task_id": "uuid",
  "agent_id": "coder-primary",
  "goal": "string",
  "context_refs": ["memory:<id>", "graph:<kind>:<id>", "file:<path>"],
  "tools_allowed": ["read_file", "search_project", "create_diff"],
  "model_policy": { "preferred": "local-coder", "fallback": "local-fallback" },
  "output_contract": "diff_proposal" | "answer" | "plan"
}

// Response
{
  "task_id": "uuid",
  "status": "ok" | "error" | "needs_approval",
  "summary": "string",
  "artifacts": ["diff:<id>", "memory:<id>"],
  "requires_approval": true,
  "error": null | { "code": "string", "message": "string" }
}
```

### Approval Token

```
token = signed(uuid_v4, expires_at = now + 300s, scope = "apply_diff", diff_id)
storage: in-memory + tool_events row with status='requested'
validation: signature ok + not expired + not previously consumed + diff_id matches
on success: tool_events row status='approved' → 'applied', token marked consumed
```

### SQLite V1 Schema

```sql
CREATE TABLE memories (
  id           TEXT PRIMARY KEY,
  project_id   TEXT NOT NULL,
  agent_id     TEXT NOT NULL,
  scope        TEXT CHECK(scope IN ('project','agent','session','decision')) NOT NULL,
  content      TEXT NOT NULL,
  importance   REAL DEFAULT 0.5,
  created_at   TEXT DEFAULT (datetime('now')),
  updated_at   TEXT DEFAULT (datetime('now'))
);
CREATE VIRTUAL TABLE memories_fts USING fts5(
  id UNINDEXED, content, scope UNINDEXED, project_id UNINDEXED, agent_id UNINDEXED
);

CREATE TABLE decisions (
  id           TEXT PRIMARY KEY,
  project_id   TEXT NOT NULL,
  agent_id     TEXT NOT NULL,
  task_id      TEXT NOT NULL,
  summary      TEXT NOT NULL,
  rationale    TEXT,
  context_refs TEXT,
  created_at   TEXT DEFAULT (datetime('now'))
);

CREATE TABLE agent_state (
  agent_id     TEXT NOT NULL,
  project_id   TEXT NOT NULL,
  state_json   TEXT NOT NULL,
  updated_at   TEXT DEFAULT (datetime('now')),
  PRIMARY KEY (agent_id, project_id)
);

CREATE TABLE tool_events (
  id              TEXT PRIMARY KEY,    -- sha256(payload)
  task_id         TEXT NOT NULL,
  agent_id        TEXT NOT NULL,
  tool_name       TEXT NOT NULL,
  inputs_hash     TEXT NOT NULL,
  output_hash     TEXT,
  approval_token  TEXT,
  status          TEXT CHECK(status IN ('requested','approved','applied','rejected','rolled_back')) NOT NULL,
  created_at      TEXT DEFAULT (datetime('now'))
) STRICT;

CREATE TABLE graph_nodes (
  id            TEXT PRIMARY KEY,
  project_id    TEXT NOT NULL,
  kind          TEXT CHECK(kind IN ('file','folder','symbol','import','export')) NOT NULL,
  name          TEXT NOT NULL,
  path          TEXT,
  span_json     TEXT,
  metadata_json TEXT
);

CREATE TABLE graph_edges (
  id            TEXT PRIMARY KEY,
  project_id    TEXT NOT NULL,
  source_id     TEXT NOT NULL,
  target_id     TEXT NOT NULL,
  kind          TEXT CHECK(kind IN ('contains','imports','exports','depends_on','tested_by')) NOT NULL,
  metadata_json TEXT
);
CREATE INDEX idx_edges_target ON graph_edges(target_id);

-- Reserved for v2; created but unused in v1
CREATE TABLE relationships (
  agent_id   TEXT NOT NULL,
  target_id  TEXT NOT NULL,
  note       TEXT,
  created_at TEXT DEFAULT (datetime('now')),
  PRIMARY KEY (agent_id, target_id)
);
```

### Agent Identity (minimal)

```yaml
# soul.yaml
id: coder-primary
name: Coder Primary
role: persistent_coder
core_values: [correctness, safety, locality, traceability]

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
  - exceed_tools_allowed
must_always:
  - produce_diff_before_apply
  - log_tool_events
  - attach_task_id_to_memory_writes
approval_required_for: [file_write, file_delete, patch_apply]
priorities: [data_integrity, user_safety, correctness, performance]
```

### CLI Surface (V1)

```
skycode scan <project>
skycode ask "<task>"
skycode diff <task-id>
skycode approve <diff-id>          → emits signed token to stdout
skycode apply <approval-token>
skycode rollback <change-id>
skycode memory search "<query>"
skycode graph impact <path-or-symbol>
```

## 6. Phased Plan (12 weeks)

### Phase 0 — Canonical Freeze (Week 0–1)
Lock /docs as source of truth. Resolve any contradictions between memory-system.md, protocol.md, agent-definition.md. Define SkyCore structs, approval token contract, SQLite migrations. Initialize Rust workspace: `runtime/`, `cli/`, `agents/`, `memory/`, `graph/`, `docs/`.

**Boundary crossings:** none (design only).

**Exit gates:** zero contradictions in /docs. Every v1 feature mapped to one canonical layer. Every write op has named approval gate. No v1 task requires network.

### Phase 1 — Safe Tool Spine (Week 1–3)
Tools: `read_file`, `list_dir`, `search_project`, `git_status`, `git_diff`, `create_diff`, `apply_diff`, `rollback`. Signed approval token (UUID, TTL=300s, single-use). Append-only `tool_events`. CLI: `diff`, `approve`, `apply`, `rollback`. **No LLM yet.**

**Boundary crossings:** CLI → Orchestrator → Tools → filesystem/git. **Risk:** file mutation. **Control:** `apply_diff` rejects missing/expired/consumed/mismatched tokens; tested in isolation.

**Exit gates:** 50 simulated edit cycles produce zero unapproved writes (audited via log). Direct write paths outside `apply_diff` fail tests. Multi-file rollback verified on real git repo. Every tool call produces queryable log entry.

### Phase 2 — Memory + Graph V1 (Week 3–5, overlaps Phase 1)
SQLite migrations. FTS5 indexing. Retrieval ranked by `keyword_score * recency_decay * importance * scope_match`. Project scanner with tree-sitter for Python, TypeScript, Rust (auto-detect on scan). Graph nodes: file/folder/symbol/import/export. Edges: contains/imports/exports/depends_on/tested_by. `skycode graph impact` recursive query.

**Boundary crossings:** Orchestrator → Memory/Graph → filesystem (read-only). **Risk:** OOM on large repos. **Control:** tree-sitter incremental parsing; scanner streams nodes to SQLite.

**Exit gates:** scan persists across restart. Memory retrieval returns scoped results. Graph impact correctly identifies affected files for ≥3 real refactors. Retrieval latency <200ms on 10k memory rows. No vector DB used.

### Phase 3 — Local Inference + SkyCore (Week 5–7)
llama.cpp GGUF loader, streaming, model registry as hot-reloadable YAML (no hardcoded names in code). SkyCore serializer/deserializer. Manual model selection via config. Optional OpenAI-compatible adapter, **disabled by default**, must not affect v1 gates.

**Boundary crossings:** Agent Runtime → SkyCore → Inference Runtime → Models. **Risk:** provider format bleed. **Control:** integration tests verify provider-specific fields never reach Agent Runtime, Orchestrator, or CLI.

**Exit gates:** local model completes SkyCore request offline. Same SkyCore request shape works across local + remote adapter. Missing model fails with explicit reason. Registry hot-reload without process restart.

### Phase 4 — Persistent Coder Agent (Week 7–10)
`coder-primary` from YAML. Task loop:
```
classify → retrieve (memory + graph refs, NOT files) → SkyCore request
        → model response → diff proposal → approval request
        → apply on approved token → log decision + tool events
```
Agent state persisted; reloads on restart. CLI `skycode ask "<task>"` proposes only; never applies silently.

**Boundary crossings:** all runtime layers exercised. **Risk:** agent bypasses orchestrator for tools/models. **Control:** Agent Runtime has no filesystem permissions and no model handles; returns `diff_proposal` JSON only. Orchestrator owns all execution.

**Exit gates:** agent recalls a decision from session 1 in session 3 after two restarts. One real edit completed offline end-to-end. All edits go through diff → approval → apply → log. Context built from refs, not file dumps. Only one agent exists in runtime.

### Phase 5 — Hardening + Router (Week 10–12)
**Hardening (release blocker):**
- End-to-end regression suite covering all prior gates.
- Failure-mode tests: missing model, expired token, double-spent token, patch conflict, restart mid-task, rollback failure.
- Offline demo: full v1 flow with network disabled.
- Benchmark: graph-aware retrieval reduces context tokens ≥50% vs naive file dump on representative tasks.

**Router (lightweight, additive):**
- Task classifier maps goal → task class (read/classify/edit/reason).
- Router maps task class → registry entry → fallback chain.
- Fallback: local-primary → local-fallback → explicit failure (no silent remote).
- Telemetry to `tool_events`: latency, model used.

**Boundary crossings:** none new. **Risk:** release with hidden unsafe path. **Control:** release blocked if any gate fails.

**Exit gates:** zero unapproved writes in full suite. Offline demo passes. ≥50% context reduction confirmed. Tool + decision logs reconstruct every applied change. SQLite sufficient under measured workload. Router selects correct class on 9/10 hand-labeled samples; fallback fires correctly on simulated primary failure.

## 7. Universal Phase Gate Checklist

Applied at every phase close (Copilot V1 ritual, kept):

| Gate           | Criterion                                                  |
|----------------|------------------------------------------------------------|
| Safety         | Zero unapproved writes in full log audit                   |
| Persistence    | All state survives clean process restart                   |
| Traceability   | Every tool event queryable by task_id and timestamp        |
| Boundary       | No layer boundary crossed outside the allowed list         |
| Quality        | Phase-specific numeric threshold met                       |

## 8. Test Plan (V1)

**Unit:** approval token (creation/expiry/single-use/replay rejection); tool policy (write rejected without token); SkyCore (request/response serialization roundtrip); memory ranking (keyword + recency + importance + scope); graph node/edge construction from sample projects.

**Integration:** diff → approval → apply → rollback (single + multi-file); scan → graph impact → context build; CLI `ask` → diff generated → no apply before approval; restart → agent recalls decision and state; FTS5 retrieval scoping.

**Offline:** every v1 flow with network disabled.

**Regression:** provider-specific response fields never reach CLI; Agent Runtime cannot import filesystem or model modules directly (architectural test).

**Failure-mode:** missing model file; expired token; double-spent token; patch conflict on apply; process kill mid-task; rollback when git state changed.

## 9. Post-V1 Backlog (Gate-Locked)

Promote only after all v1 gates pass and 2 weeks of stable v1 use:

- Reviewer agent (coder proposes, reviewer critiques, human approves)
- Remote model fallback enabled by user config
- Manager/architect agents
- Tauri UI
- Vector embeddings (only if FTS5 fails measured retrieval benchmarks)
- Multi-agent orchestration

**Never:** swarm consensus, personality drift, emotional valence, voice as v1/v2 feature, "AI civilization" framing.

## 10. V1 Success Definition

User runs `skycode ask "extract auth logic into a service"` on a 50k-line codebase, network disabled.

The agent:
1. Reads the graph (no full-file dump).
2. Proposes a unified diff.
3. Waits for explicit approval (`skycode approve <diff-id>` → token).
4. Applies via `skycode apply <token>`.
5. Logs decision with rationale.
6. Three sessions later, references *why* it made that change.

Zero unapproved writes in any log. Context tokens ≤50% of naive baseline. 100% local execution.

That's v1. Reviewer, UI, multi-agent, and everything DeepSeek dreamed about come after.
