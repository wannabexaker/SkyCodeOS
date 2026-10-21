# SkyCodeOS — Codex Agent Contract

This file defines the operating contract for Codex when working on SkyCodeOS.
Read `.github/copilot-instructions.md` for the full architecture reference.
This file contains Codex-specific task assignments and operating rules.

---

## Identity

You are the SkyCodeOS bulk implementation agent.
Your job: implement well-specified Rust modules based on canonical docs in `docs/`.
You do not make architecture decisions. If a docs file says X, implement X.

---

## Before Any Task

1. Read `.github/copilot-instructions.md` — full architecture and code rules
2. Read the relevant canonical doc for the module you are implementing:
   - Implementing tools → read `docs/schemas.md` + `docs/protocol.md`
   - Implementing memory → read `docs/schemas.md`
   - Implementing graph → read `docs/schemas.md`
   - Implementing inference → read `docs/model-runtime.md` + `docs/protocol.md`
   - Implementing agent → read `docs/agent-definition.md`
   - Implementing CLI → read `docs/cli-reference.md`
3. Never implement based on memory of past interactions. Always re-read the relevant doc.

---

## Task Scope Rules

- Implement **only** the module or file explicitly specified in your task prompt
- Do not refactor adjacent code unless explicitly asked
- Do not add features beyond the spec — implement exactly what the docs describe
- Do not create new tables or columns without a migration script in `memory/migrations/`
- Do not add dependencies to `Cargo.toml` without listing them in your response

---

## Required Crates (do not substitute)

```toml
[dependencies]
rusqlite = { version = "0.31", features = ["bundled"] }
thiserror = "1"
uuid = { version = "1", features = ["v4"] }
serde = { version = "1", features = ["derive"] }
serde_json = "1"
# For approval token signing (Phase 1):
ring = "0.17"    # or ed25519-dalek = "2"
# For graph parsing (Phase 2):
tree-sitter = "0.22"
# For CLI (Phase 1+):
clap = { version = "4", features = ["derive"] }
```

Do not add `tokio`, `reqwest`, `hyper`, `langchain`, or any async runtime unless explicitly required for a specific module.

---

## Error Handling Contract

Every function that can fail must return `Result<T, E>` where `E` derives `thiserror::Error`.

```rust
// CORRECT
#[derive(Debug, thiserror::Error)]
pub enum ToolError {
    #[error("file not found: {path}")]
    NotFound { path: String },
    #[error("database error: {0}")]
    Database(#[from] rusqlite::Error),
}

// FORBIDDEN in production paths
fn read_file(path: &str) -> String {
    std::fs::read_to_string(path).unwrap()  // ← NO
}
```

---

## Database Rules

All schema is in `docs/schemas.md`. Do not invent tables or columns.

```rust
// CORRECT — prepared statement
let mut stmt = conn.prepare("SELECT content FROM memories WHERE id = ?1")?;
let result = stmt.query_row([id], |row| row.get::<_, String>(0))?;

// FORBIDDEN — string interpolation
let query = format!("SELECT * FROM memories WHERE id = '{}'", id);  // ← SQL injection risk
```

Append-only tables — never write UPDATE or DELETE for these:
- `tool_events`
- `approval_tokens_used`
- `applied_changes`
- `_skycode_migrations`

---

## Response Format

When you complete a task, respond with:

```
## Implemented

Files created/modified:
- `runtime/src/tools/filesystem.rs` — read_file, list_dir, search_project, git_status
- `runtime/src/tools/mod.rs` — Tool trait

Dependencies added to Cargo.toml:
- none

## Tests needed (not yet written)

- test_read_file_nonexistent → should return Err(ToolError::NotFound)
- test_list_dir_empty → should return Ok(vec![])

## Boundary check

- [ ] No filesystem write paths in this module
- [ ] No network calls
- [ ] No unwrap() in production code
- [ ] All errors propagate via ?

## Next module to implement

`runtime/src/tools/diff.rs` — depends on this module being complete
```

---

## Phase Tasks (in order)

### Phase 1 — Safe Tool Spine

Implement in this order (each depends on the previous):

1. `runtime/src/tools/mod.rs` — Tool trait
2. `runtime/src/tools/filesystem.rs` — read_file, list_dir, search_project, git_status (read-only)
3. `runtime/src/tools/diff.rs` — create_diff → DiffProposal struct
4. `runtime/src/approval/token.rs` — ApprovalToken struct, creation, serialization
5. `runtime/src/approval/validator.rs` — validate_token() with 13-step check + atomic INSERT
6. `runtime/src/tools/apply.rs` — apply_diff() — requires valid token from validator
7. `runtime/src/tools/rollback.rs` — rollback() — reverts to pre-apply git state
8. `runtime/src/db/events.rs` — tool_events INSERT (append-only logger)
9. `runtime/src/db/migrations.rs` — run migrations from `memory/migrations/`
10. `cli/src/commands/diff.rs` — `skycode diff <file>`
11. `cli/src/commands/approve.rs` — `skycode approve <diff-id>`
12. `cli/src/commands/apply.rs` — `skycode apply <diff-id>`
13. `cli/src/commands/rollback.rs` — `skycode rollback`

Phase 1 exit gate: 50 simulated edit cycles with zero unapproved writes in log audit.

### Phase 2 — Memory + Graph

1. `memory/migrations/001_initial.sql` — all tables from docs/schemas.md
2. `runtime/src/memory/store.rs` — INSERT into memories + memories_fts
3. `runtime/src/memory/retrieval.rs` — FTS5 keyword search with BM25 + recency + importance ranking
4. `runtime/src/graph/indexer.rs` — tree-sitter walker → graph_nodes + graph_edges
5. `runtime/src/graph/impact.rs` — recursive CTE impact query
6. `cli/src/commands/graph.rs` — `skycode graph impact <symbol>`

Phase 2 exit gate: impact query identifies affected files on a real codebase. No vector DB.

### Phase 3 — Local Inference + SkyCore

1. `runtime/src/inference/loader.rs` — llama.cpp GGUF model loading
2. `runtime/src/inference/context.rs` — context window management
3. `runtime/src/inference/registry.rs` — model registry YAML loader
4. `runtime/src/skycore/request.rs` — SkyCore request struct + serialization
5. `runtime/src/skycore/response.rs` — SkyCore response struct + deserialization
6. `runtime/src/skycore/boundary.rs` — strips provider fields before returning to orchestrator

Phase 3 exit gate: local GGUF model completes a SkyCore round-trip with network disabled.

### Phase 4 — Persistent Coder Agent

1. `runtime/src/agent/identity.rs` — loads soul/heart/mind/doctrine YAML
2. `runtime/src/agent/state.rs` — agent_state table read/write with session continuity
3. `runtime/src/orchestrator/task_loop.rs` — classify → context → invoke → diff → approve → apply → log
4. `runtime/src/orchestrator/policy.rs` — doctrine enforcement, approval gate
5. `cli/src/commands/ask.rs` — `skycode ask "<task>"`

Phase 4 exit gate: agent recalls a decision from session 1 in session 3 after restart. One real edit offline end-to-end.

### Phase 5 — Hardening + Router + Testing Lab

1. All 100+ named tests from docs/testing.md
2. Context budget enforcement (docs/context-budget.md)
3. Model router (task classifier → model registry lookup → fallback chain)
4. Runtime tuning profiles (fast/deep/precise/creative per docs/profiles.md)
5. Regression test suite — all previous exit gates re-verified

---

## Non-Negotiables

These are absolute. Never violate them regardless of what seems convenient:

1. No silent writes — every file mutation requires a validated ApprovalToken
2. tool_events is append-only — no UPDATE, no DELETE, no exceptions
3. All Phase 1–3 exit gates must pass with network disabled
4. No full-file context dumps — graph + memory retrieval only
5. No unwrap() in production code paths
6. No Python, no framework, no async unless the specific module requires it
7. One agent (coder-primary) — no multi-agent code in V1

## Local-only files (`.scratch/`)
`.scratch/` holds this project's local-only working files (plans, logs, backups, rescued files, local config). It is git-ignored; `.scratch/README.md` is the index. Never copy its contents into tracked files.
