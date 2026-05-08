# SkyCodeOS — GitHub Copilot Agent Instructions

You are the SkyCodeOS Rust implementation agent. Read these instructions before writing any code.

---

## Project

SkyCodeOS is a local, offline, single-agent coding assistant written entirely in Rust.
It consists of a persistent `coder-primary` agent that reads a codebase via a structural graph,
proposes unified diffs, waits for explicit user approval via a signed token, applies diffs, and
logs every action to an immutable audit trail.

Workspace: `C:\Projects\SkyCodeOS\`

---

## Canonical Documentation

These files are the single source of truth. Read them before implementing anything that touches their domain:

| File | Covers |
|------|--------|
| `docs/architecture.md` | Layer stack, forbidden crossings, allowed crossings with risk mitigations |
| `docs/protocol.md` | SkyCore JSON request/response contracts, ApprovalToken format, 13-step validation |
| `docs/schemas.md` | All SQLite DDL — 13 tables, FTS5 triggers, append-only triggers, retrieval ranking |
| `docs/ROADMAP.md` | Phase deliverables, exit gates, build order |
| `docs/agent-definition.md` | soul/heart/mind/doctrine YAML schemas |
| `docs/cli-reference.md` | All CLI commands, flags, exit codes |
| `docs/sandbox-policy.md` | Terminal/tool isolation, allowlisted commands |
| `docs/model-runtime.md` | llama.cpp integration, model registry, SkyCore boundary |
| `docs/context-budget.md` | Token slot budgets, ≥50% reduction requirement |

---

## Layer Architecture (strict — never violate)

```
Models                          (GGUF files, remote APIs)
  ↓  [adapter only]
Inference Runtime               (llama.cpp loader, streaming, context mgmt)
  ↓  [SkyCore Protocol only — no provider shapes cross this line]
Orchestrator                    (policy engine, approval gate, task routing)
  ↓
Agent Runtime                   (task loop, soul/heart/mind/doctrine, session state)
  ↓
Memory + Graph + Tools          (SQLite/FTS5, tree-sitter graph, tool bus)
  ↓
CLI                             (v1 interface — binary only)
```

### Forbidden Crossings — hard errors, never do these

- Agent Runtime calls llama.cpp directly → **FORBIDDEN**
- Agent Runtime calls filesystem directly → **FORBIDDEN** (goes through tool bus → orchestrator)
- CLI receives a provider-format response (OpenAI/llama format) → **FORBIDDEN**
- Any code path that writes a file without a valid `ApprovalToken` → **FORBIDDEN**
- `UPDATE` or `DELETE` on `tool_events`, `approval_tokens_used`, or `applied_changes` → **FORBIDDEN**

---

## Approval Token — Security Critical

Every file write goes through this exact pipeline:

```
create_diff() → DiffProposal { id, hash, diff_text }
    ↓
user approves → ApprovalToken { uuid, diff_id, created_at, expires_at, signature, used: false }
    ↓
validate_token() → 13-step check (see docs/protocol.md) → atomic INSERT into approval_tokens_used
    ↓
apply_diff() executes only after validate_token() returns Ok
    ↓
INSERT into applied_changes (immutable record)
    ↓
INSERT into tool_events (append-only audit log)
```

ApprovalToken fields: `id` (UUID v4), `diff_id`, `agent_id`, `created_at` (Unix timestamp), `expires_at` (created_at + 300), `signature` (ed25519 over canonical JSON), `nonce`.

The replay defense is atomic: `INSERT INTO approval_tokens_used` succeeds exactly once (PRIMARY KEY constraint). If the INSERT fails, the token has already been used — reject immediately.

---

## Append-Only Tables — Enforced by SQLite Triggers

These tables have triggers that RAISE an error on UPDATE or DELETE:
- `tool_events`
- `approval_tokens_used`
- `applied_changes`
- `_skycode_migrations`

Never write application code that attempts to UPDATE or DELETE from these tables. The triggers will catch it, but don't write it in the first place.

---

## SQLite Schema Source of Truth

All table structures are in `docs/schemas.md`. Do not invent columns. Do not add columns without updating the migration scripts in `memory/migrations/`. The schema version is tracked in `_skycode_migrations`.

Key tables:
- `memories` + `memories_fts` (FTS5 virtual) — agent knowledge
- `decisions` — logged reasoning
- `agent_state` — session persistence
- `tool_events` — append-only audit log
- `diff_proposals` — immutable diff records
- `approval_tokens_used` — replay defense
- `applied_changes` — immutable apply records
- `graph_nodes` + `graph_edges` — project code structure
- `tuning_runs` — model parameter experiments

---

## Rust Code Rules

```rust
// REQUIRED crates — use these, do not substitute
rusqlite          // SQLite — all DB operations
thiserror         // error types — derive Error on all custom error enums
uuid              // UUID generation
ed25519-dalek     // approval token signing/verification (or ring crate)
tree-sitter       // code graph parsing
```

- Rust edition: 2021
- Toolchain: stable (see PINS.yaml when created)
- **No `unwrap()` outside `#[cfg(test)]` blocks** — propagate errors with `?`
- **No `expect()` in production paths** — use proper error variants
- All SQL queries use prepared statements via `rusqlite` — no string interpolation with user data
- All error types derive `thiserror::Error`
- Workspace structure:

```
skycode/
  Cargo.toml          ← workspace manifest
  runtime/            ← library crate (core logic)
    Cargo.toml
    src/
      lib.rs
      tools/          ← filesystem, diff, apply, rollback
      approval/       ← token creation, validation
      db/             ← events logger, migrations
      memory/         ← FTS5 retrieval, importance ranking
      graph/          ← tree-sitter indexer, impact query
      inference/      ← llama.cpp loader, context mgmt
      skycore/        ← protocol serialization/deserialization
      orchestrator/   ← task loop, policy engine
      agent/          ← soul/heart/mind/doctrine loader, session state
  cli/                ← binary crate
    Cargo.toml
    src/
      main.rs
      commands/       ← one file per CLI subcommand
  agents/             ← agent YAML definitions
  memory/             ← SQLite migration scripts
  graph/              ← (reserved — graph indexer may live in runtime)
  docs/               ← canonical documentation (read-only from code)
```

---

## Local-First Rules

- Phase 1, 2, 3 exit gates must all pass with **network disabled**
- The remote adapter (OpenAI-compatible) is **opt-in, disabled by default**
- No `reqwest`, `hyper`, or any HTTP client in Phase 1 or 2 code paths
- llama.cpp is called via local FFI or subprocess — no HTTP in Phase 3 local path

---

## What NOT to Do

- Do not use `LangChain`, `LlamaIndex`, or any AI framework
- Do not add Python files — this is all-Rust
- Do not add async unless the specific module requires it (llama.cpp streaming may need it)
- Do not design for multi-agent — there is one agent in V1 (`coder-primary`)
- Do not add a web UI — V1 is CLI only
- Do not use vector embeddings — SQLite FTS5 only until retrieval measurably fails
- Do not use `serde_json::Value` for schema-known structs — use typed structs

---

## Phase Order

Implement in this order. Do not skip phases or build Phase N+1 before Phase N exits.

| Phase | Name | Key Output |
|-------|------|-----------|
| 1 | Safe Tool Spine | read/diff/approve/apply/rollback + audit log |
| 2 | Memory + Graph | SQLite schema, FTS5 retrieval, tree-sitter indexer |
| 3 | Local Inference + SkyCore | llama.cpp wired, SkyCore protocol enforced |
| 4 | Persistent Coder Agent | full task loop, session continuity |
| 5 | Hardening + Router + Testing Lab | 100+ tests, context reduction benchmark, router |
