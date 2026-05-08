# GithubCopilot Plan — SkyCodeOS v2

---

## A. V1 Self-Critique (What Was Wrong in My v1)

| Issue | v1 mistake | Fix in v2 |
|---|---|---|
| Over-scoped v1 | Model router, fallback chain, relationship memory, multi-agent all listed as v1 | Cut everything past single-agent reliability |
| Orchestration pipeline listed "agent selection" | Implies multiple agents; layer violation for v1 | Step removed; single agent, no selection needed |
| "Reviewer in v1.5" | Undefined version; not a gate | Reviewer is Phase 5, explicitly post-v1 |
| Phase E (week 15-24) inside "v1 roadmap" | Desktop UX + multi-agent are v2 | Phase E is now flagged v2, not v1 |
| Approval as implicit boolean | Replay-vulnerable | Adopt Claude's signed-UUID-with-TTL token |
| Graph and model runtime bundled in one phase | Too much; graph should land before router | Split into Phase 4 (graph) and Phase 5 (runtime + router) |
| "Global memory" scope listed | Cross-agent, not needed in single-agent v1 | Removed from v1 tables |
| Relationship memory in v1 | Multi-agent concept | Deferred; schema reserved for v2 migration |
| Vague exit criteria ("significantly reduces") | Not testable | All gates now have numeric thresholds |

---

## B. Ideas Adopted from Other Plans (with attribution)

| Idea | Source | Why |
|---|---|---|
| "useful → intelligent → social → alive" sequencing | ChatGPT | Correct build order; prevents over-engineering |
| `skills.yaml` separate from `tools.yaml` | ChatGPT | Skills = behavioral patterns; tools = executables. Different things |
| `doctrine.yaml` with numbered priority order | DeepSeek | Makes conflict resolution deterministic |
| Complete SQL schema — `knowledge_edges`, `episodic_memory` as future migration targets | DeepSeek | Avoids schema churn when upgrading; design now, activate later |
| Tool definition with `safety: requires_approval` and `rollback: git_branch` inline | DeepSeek | Approval intent expressed at the tool contract level |
| Approval token = signed UUID with TTL | Claude | Prevents replay attacks and stale approvals |
| Tool event log = append-only, content-addressable | Claude | Audit spine; non-negotiable |
| tree-sitter for parsing, not custom | Claude | Battle-tested; incremental; no reinvention |
| SQLite FTS5 before embeddings — with a measured benchmark gate | Claude | Forces evidence-based upgrade, not premature optimization |
| Phase 0 = freeze /docs before any code | Claude | Single source of truth before implementation diverges |
| 50-edit stress gate on safe-write pipeline | Claude | Concrete, not subjective |
| HITL framing as "human-in-the-loop" HITL gate | Gemini | Cleaner framing for approval requirement |
| Guardrail named "No Context Dumping" | Gemini | Makes the anti-pattern explicit and named |

---

## C. Positions from V1 I Still Hold

**12-16 week baseline is right.** Claude converges on 12 weeks for v1. DeepSeek's 24-week plan includes multi-agent and UI which are v2. ChatGPT's 10 phases collapse to the same four v1 phases. The range is honest about uncertainty; compressing to 8 weeks is wishful.

**Phase overlap (B/C, C/D) is valid.** Tool spine does not block agent work once the write-lock is proven. Parallelising is risk-managed, not chaotic. The gates enforce sequential validation even when development overlaps.

**All-Rust runtime is cleaner.** ChatGPT's Python + Rust split works but tempts LangChain-flavoured dependency drift. Python is faster to prototype but the protocol boundary is harder to enforce. All-Rust with defined FFI bindings if needed. Not negotiable past Phase 3.

---

## D. Layer Map (frozen, per /docs/architecture.md)

```
Models
  ↓  [adapter only]
Inference Runtime  (llama.cpp + OpenAI-compatible remote adapter)
  ↓  [SkyCore Protocol only — no provider shapes cross this boundary]
Agent Runtime  (task loop, soul/heart/mind/doctrine, session state)
  ↓
Memory + Graph + Tools  (SQLite, FTS5, tree-sitter graph, tool bus)
  ↓
Orchestrator  (policy engine, approval gate, event log)
  ↓
CLI  (v1 interface)
  ↓  [v2+]
Tauri UI
```

**Boundary rules (violations flagged with ⚠️ in phase tasks below):**
- UI never receives provider-format responses.
- Agent Runtime never calls Inference Runtime directly; only via SkyCore.
- Every write-capable tool call passes through policy engine first.
- Memory is never queried from UI layer directly.

---

## E. Agent Definition (V1 — minimal)

Directory layout (from ChatGPT, trimmed to v1 fields):

```
/agents/coder-primary/
  /core/
    soul.yaml
    heart.yaml
    mind.yaml
    doctrine.yaml
  /capabilities/
    skills.yaml      # behavioral patterns — not executables
    tools.yaml       # executable capabilities with safety contracts
    permissions.yaml
  /state/
    current.yaml     # active task, status
  memory.sqlite      # scoped to this agent
```

**soul.yaml (v1 minimum):**
```yaml
id: coder-primary-v1
name: Coder
role: Software Engineer
core_values: [correctness, safety, traceability]
```

**doctrine.yaml (v1 minimum, DeepSeek format with priority order):**
```yaml
must_never:
  - write files without an approved diff token
  - force push git history
  - run terminal commands outside the allowed list
  - exceed tool permissions defined in permissions.yaml
must_always:
  - generate a diff before any file modification
  - log every tool execution to the event log
  - attach task_id to every memory write
priorities:
  1: data_integrity
  2: user_safety
  3: correctness
  4: performance
approval_required_for:
  - file writes
  - git commits
  - terminal commands
```

**tools.yaml (DeepSeek contract format):**
```yaml
tools:
  - name: filesystem_read
    capabilities: [read_file, list_directory, search_content]
    constraints: [no_write, respect_gitignore]
  - name: code_editor
    capabilities: [create_file, edit_file, delete_file]
    safety: requires_approval          # ← approval token validated before execution
    rollback: git_branch
  - name: terminal_executor
    capabilities: [run_command, get_output]
    sandbox: process_isolation
    timeout_seconds: 30
    allowed_commands: [cargo, git, make, npm, pip, pytest, tsc]
```

---

## F. Memory Schema (V1 tables — SQLite FTS5)

```sql
-- Active v1 tables
CREATE TABLE memories (
    id       TEXT PRIMARY KEY,
    agent_id TEXT NOT NULL,
    project_id TEXT,
    scope    TEXT NOT NULL, -- 'project' | 'agent' | 'session' | 'decision'
    content  TEXT NOT NULL,
    tags     TEXT,          -- space-separated for FTS5
    importance REAL DEFAULT 0.5,
    created_at INTEGER NOT NULL,
    last_accessed INTEGER
);

CREATE VIRTUAL TABLE memories_fts USING fts5(content, tags, content=memories);

CREATE TABLE decisions (
    id         TEXT PRIMARY KEY,
    agent_id   TEXT NOT NULL,
    project_id TEXT NOT NULL,
    task_id    TEXT NOT NULL,
    summary    TEXT NOT NULL,
    rationale  TEXT,
    outcome    TEXT,         -- 'approved' | 'rejected' | 'rolled_back'
    created_at INTEGER NOT NULL
);

CREATE TABLE tool_events (
    id          TEXT PRIMARY KEY,  -- content-addressable: sha256 of payload
    task_id     TEXT NOT NULL,
    agent_id    TEXT NOT NULL,
    tool_name   TEXT NOT NULL,
    inputs      TEXT NOT NULL,     -- JSON
    output      TEXT,
    approval_token TEXT,           -- signed UUID with TTL; NULL for read-only tools
    status      TEXT NOT NULL,     -- 'ok' | 'rejected' | 'rolled_back'
    created_at  INTEGER NOT NULL
) STRICT;
-- ↑ append-only; no UPDATE or DELETE permitted via application layer

CREATE TABLE agent_state (
    agent_id    TEXT PRIMARY KEY,
    current_task TEXT,
    status      TEXT,
    session_id  TEXT,
    updated_at  INTEGER NOT NULL
);

-- Reserved for v2; not activated in v1
-- knowledge_edges, relationship_memory, episodic_memory (DeepSeek schemas)
```

Retrieval ranking (v1): `importance * recency_decay * scope_match`. No embeddings. FTS5 keyword search first.

---

## G. Graph Schema (V1 — structural only)

```sql
CREATE TABLE graph_nodes (
    id       TEXT PRIMARY KEY,   -- sha256(type + path + name)
    project_id TEXT NOT NULL,
    type     TEXT NOT NULL,      -- 'file' | 'function' | 'class' | 'module'
    path     TEXT NOT NULL,
    name     TEXT,
    language TEXT,
    updated_at INTEGER NOT NULL
);

CREATE TABLE graph_edges (
    from_id  TEXT NOT NULL,
    to_id    TEXT NOT NULL,
    type     TEXT NOT NULL,      -- 'imports' | 'calls' | 'contains' | 'depends_on'
    PRIMARY KEY (from_id, to_id, type)
);

CREATE INDEX idx_edges_to ON graph_edges(to_id);
```

Parser: tree-sitter (not custom). Languages in v1: whatever the user's actual codebase uses — detect via file extension, configure lazily.

Impact query (required for gate):
```sql
-- "What nodes depend on X?"
WITH RECURSIVE deps(id) AS (
  SELECT from_id FROM graph_edges WHERE to_id = ?
  UNION ALL
  SELECT e.from_id FROM graph_edges e JOIN deps d ON e.to_id = d.id
)
SELECT * FROM graph_nodes WHERE id IN (SELECT id FROM deps);
```

---

## H. Phased Roadmap (12-16 weeks, v1 only)

### Phase 0 — Freeze (Week 0-1)
**Goal:** Single source of truth before any code.

- Lock /docs as canonical. Resolve conflicts between docs files if any exist.
- Confirm v1 scope boundary: single agent, no UI, no multi-agent, no router in Phase 0-3.
- Initialize git repo, monorepo structure, Rust workspace.

```
skycode/
  runtime/     # Rust — core runtime, tool bus, policy engine
  agents/      # agent definition files
  memory/      # schema migrations
  graph/       # tree-sitter indexer
  cli/         # CLI binary
  docs/        # canonical docs
```

**Exit gate:** `/docs` has zero unresolved contradictions. Repo structure committed.

---

### Phase 1 — Tool Spine (Week 1-3)
**Goal:** Safe read + write pipeline works on real repos. No LLM yet.

Build:
- `read_file`, `list_dir`, `search_project` (grep-level)
- `git_status`, `git_diff`
- `create_diff` (unified diff output)
- Approval gate: signed UUID token with TTL=300s, verified before any write
- `apply_diff` — only executes with valid token
- `rollback` — reverts to pre-apply git state
- Append-only `tool_events` log (content-addressable rows)

⚠️ No LLM calls in this phase. Tool bus must be testable in isolation.

**Exit gate:** 50 simulated edit cycles (propose → approve → apply → rollback) with zero unapproved writes detected by log audit.

---

### Phase 2 — Local Inference (Week 3-5)
**Goal:** llama.cpp runs locally; SkyCore protocol shapes every call.

Build:
- llama.cpp GGUF loader, streaming, context management
- One OpenAI-compatible remote adapter (opt-in only)
- Model registry YAML (hot-reloadable, no hardcoded names in code)
- SkyCore request/response serialization (per `/docs/protocol.md`)
- No router yet — model selection is manual via config

⚠️ Agent Runtime must never call llama.cpp directly. SkyCore boundary enforced here.

**Exit gate:** Same SkyCore request produces structurally identical response shape from both local GGUF and remote adapter. Registry reload without process restart.

---

### Phase 3 — Single Coder Agent (Week 5-8)
**Goal:** One persistent agent with memory, tool access, session continuity.

Build:
- `coder-primary` agent: soul/heart/mind/doctrine (minimal fields per Section E)
- Task loop: receive goal → retrieve context → plan → propose → await approval → execute → log
- SQLite memory: `memories`, `decisions`, `agent_state` tables (Section F)
- FTS5 keyword retrieval with importance + recency ranking
- Session resume: agent reloads `agent_state` and top-N recent `memories` on start

⚠️ No "agent selection" logic. There is one agent. Orchestrator routes tasks to it directly.

**Exit gate:** Agent recalls a specific decision from session 1 (by keyword) during session 3, after two full process restarts. Retrieval latency <200ms on 10k memory rows.

---

### Phase 4 — Graph V1 (Week 8-10)
**Goal:** Agent retrieves context from graph instead of loading files.

Build:
- tree-sitter indexer for detected project languages
- `graph_nodes` + `graph_edges` tables (Section G)
- Context builder: for a given task, resolve relevant nodes via graph traversal, pass refs instead of file contents
- Impact query: "what breaks if I change X" (recursive edge traversal)
- `tool_events` linked to `graph_nodes` where applicable

**Exit gate:** Graph-based context reduces tokens passed to model by ≥50% compared to naive full-file load, measured on a real edit task of 3+ files.

---

### Phase 5 — Model Router (Week 10-12)
**Goal:** Task type drives model selection automatically.

Build:
- Task classifier: maps goal text → task class (read, classify, edit, reason)
- Router: task class → model registry lookup → fallback chain
- Fallback chain: local-primary → local-fallback → remote-strong → explicit failure (never silent)
- Telemetry: inference latency, cost estimate, model used — written to `tool_events`

**Exit gate:** Router selects correct model class for 9/10 hand-labelled task samples. Fallback fires and recovers on simulated primary model failure.

---

### V2 Backlog (not in scope for v1)

| Item | Source plan | Trigger to promote |
|---|---|---|
| Reviewer agent | Claude, ChatGPT | Phase 5 gate passes + v1 stability over 2 weeks |
| Desktop UI (Tauri + React) | All plans | Reviewer agent working |
| Vector embeddings + semantic search | DeepSeek, Claude | FTS5 measurably failing on retrieval benchmarks |
| Multi-agent orchestration | All plans | Reviewer + Coder stable together |
| Relationship memory + trust scores | DeepSeek | Multi-agent proven |
| Personality drift, emotional valence | DeepSeek | Post v2 |
| Voice / multimodal | DeepSeek | Post v2 |
| Swarm / consensus | DeepSeek | Never in v1 or v2 |

---

## I. Universal Phase Gate Checklist

Applied at every phase close:

| Gate | Criterion |
|---|---|
| Safety | Zero unapproved writes in full log audit |
| Persistence | All state survives clean process restart |
| Traceability | Every tool event queryable by task_id and timestamp |
| Boundary | No layer boundary crossed (UI↔provider, Agent↔provider direct) |
| Quality | Phase-specific numeric threshold met (defined above per phase) |

---

## J. V1 Success Definition

User runs `skycode task "extract auth logic into a service"` on a real codebase.

The agent:
1. Reads the graph — no full-file dump
2. Proposes a unified diff
3. Waits for explicit approval
4. Applies the patch via approved token
5. Logs the decision with rationale
6. Three sessions later, references why it made that change

No remote API required. No unintended write ever in the log. Token usage ≤50% of naive baseline.

