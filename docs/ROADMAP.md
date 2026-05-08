# SkyCodeOS — Phased Roadmap

V1: 12-week execution plan. Source: `docs/Plans/ClaudePlanMaster.md §6`.
V2 (Phase 6): 6-week extension on top of shipped V1.
Precedence: this file wins over the master plan on any factual conflict.

**Current status:** V1 SHIPPED (Phases 0–5 CLOSED) · Phase 6 OPEN — planning/scoping

---

## Phase Status Overview

| Phase | Name | Weeks | Status |
|-------|------|-------|--------|
| 0 | Canonical Freeze | 0–1 | ✅ CLOSED (partial docs — deferred to Phase 5) |
| 1 | Safe Tool Spine | 1–3 | ✅ CLOSED — gate passed |
| 2 | Memory + Graph | 3–5 | ✅ CLOSED — all gates passed |
| 3 | Local Inference + SkyCore | 5–7 | ✅ CLOSED — all gates passed |
| 4 | Persistent Coder Agent | 7–10 | ✅ CLOSED — all gates passed |
| 5 | Hardening + Router + Testing Lab | 10–12 | ✅ CLOSED — all gates passed |
| 6 | Agentic Edit Loop + Hybrid Inference | 12–18 | 🟡 OPEN — planning |

---

## Phase 0 — Canonical Freeze (Week 0–1) ✅

**Goal:** Lock `/docs`. Initialize Rust workspace. No product behavior.

**Deliverables — built:**
- Master plan committed (`docs/Plans/ClaudePlanMaster.md`)
- Rust workspace initialized with `runtime/` and `cli/` crates
- Canonical docs created: `architecture.md`, `protocol.md`, `schemas.md`, `roadmap.md`

**Pending Phase 0 docs (Claude writing these):**
- `docs/agent-definition.md` — soul/heart/mind/doctrine schemas
- `docs/cli-reference.md` — all CLI commands and exit codes
- `docs/sandbox-policy.md` — terminal/tool isolation rules
- `docs/secrets-privacy.md` — redaction rules
- `docs/config-hierarchy.md` — global/project/policy/agent config
- `docs/trust-model.md` — project trust and untrusted-mode behavior
- `docs/PINS.yaml` — pinned toolchain/deps
- `docs/git-isolation.md` — branch strategy, HEAD preservation
- `docs/context-budget.md` — per-slot budgets, ≥50% reduction rule
- `docs/migrations.md` — migration versioning and backup
- `docs/model-runtime.md` — llama.cpp, model registry, hardware tiers
- `docs/testing.md` — 100+ named tests
- `docs/success-criteria.md` — V1 success definition
- `docs/profiles.md` — tuning profiles (fast/deep/precise/creative)

**Exit gates:**
- [ ] Zero contradictions in `/docs`
- [ ] Every V1 feature mapped to one canonical layer
- [ ] Every write-capable operation has a named approval gate
- [ ] No V1 task requires the network
- [ ] `cargo test -p skycode-core -p skycode-protocol` round-trip passes

**Known deviation:** Workspace uses `runtime/` + `cli/` instead of the canonical per-responsibility crate split (`skycode-core`, `skycode-tools`, `skycode-memory`, etc.). Forbidden cross-dependencies currently enforced by convention only. Crate restructure is Phase 5 hardening work.

---

## Phase 1 — Safe Tool Spine (Week 1–3) ✅ GATE PASSED

**Goal:** Safe edit pipeline on real repos with no LLM.

**Deliverables — built:**
- Read tools: `read_file`, `list_dir`, `search_project`, `git_status`
- `create_diff` → `DiffProposal` (id = sha256 content-addressed)
- `apply_diff(diff_id, token)` — validates per `docs/protocol.md`, applies via `git apply`
- `rollback` — reverts via `git checkout`
- Approval token: ed25519, UUID v4, TTL=300s, single-use via atomic INSERT into `approval_tokens_used`
- 13-step token validation with `AgentMismatch` + `ReplayDetected` + `DiffBindingMismatch` variants
- `tool_events` append-only logger with `EventType` enum (20 variants matching schema)
- `content_id()` sha256 helper for content-addressable event IDs
- `migrations.rs` — versioned, idempotent, sha256-recorded
- CLI: `skycode diff`, `skycode approve` (stub), `skycode apply` (stub), `skycode rollback`

**Stubs (wired in Phase 4):**
- `approve` CLI — needs key management from agent state
- `apply` CLI — needs diff_proposals DB lookup + orchestrator routing

**Exit gate — PASSED:**
- ✅ 50 simulated edit cycles: 0 unapproved writes (`phase1_gate_50_edit_cycles_zero_unapproved_writes`)
- ✅ tool_events: 50 rows all `diff_applied`, zero with null `approval_token_id`
- ⬜ Red-team: no public write path other than `apply_diff` (verify in Phase 5)
- ⬜ Multi-file rollback on real repo (verify in Phase 5 regression)
- ⬜ UPDATE/DELETE triggers fire (verify when migrations run in Phase 2)

---

## Phase 2 — Memory + Graph V1 (Week 3–5) ✅ CLOSED

**Goal:** Retrieval substrate. Context without full-file dumps.

**Built:**
- All 13 tables from `docs/schemas.md` applied via `memory/migrations/001_initial.sql`
- Memory write API: scope-tagged inserts with `task_id` binding
- Memory retrieval: ranked by `bm25 * recency_decay * importance * scope_match` (FTS5)
  - FTS5 query sanitiser: strips special chars to prevent syntax errors
- Project scanner: tree-sitter (Rust, TypeScript, Python); incremental on mtime+size
- Graph nodes/edges: file, symbol, import kinds; contains/imports/calls/depends_on edges
- `calls` edges: function-level cross-symbol call graph from tree-sitter AST walking
- `scos graph impact <symbol>` — recursive CTE, symbol-preferring name lookup
- `scos scan --force` — clears project graph and does full rescan
- Known limitation: cross-module dispatcher pattern (match arms) has partial edge coverage

**Deferred to Phase 5:**
- `.git/HEAD` watcher (adds complexity, low priority vs manual `scos scan`)

**Exit gates — all passed:**
- ✅ Scan persists across restart
- ✅ Memory retrieval scoped correctly (project_id + agent_id filters, scope_match scoring)
- ✅ Graph impact correct on ≥3 real refactors (hello→main, apply_diff_from_store, search_memories)
- ✅ No vector DB / embeddings / remote service used
- ✅ Memory and graph modules have no inference dependency

---

## Phase 3 — Local Inference + SkyCore (Week 5–7) ✅ CLOSED

**Goal:** Local llama.cpp behind SkyCore. Hot-reloadable model registry.

**Built:**
- llama-server HTTP wrapper (OpenAI-compatible; port configurable per model entry)
- Runtime flags: `--n-gpu-layers`, `--ctx-size`, `--threads`, `--n-cpu-moe`, `--no-mmap`, `--mlock`
- mlock verification post-launch; line-stream reader for stdout/stderr
- Health polling: 500ms interval, 300s timeout (CPU model takes 1–3 min to load)
- Model registry loader (`agents/models.yaml`); file-watcher hot reload without restart
- CLI: `scos model load`, `scos model verify`, `scos model bench`
- SkyCore serializer/deserializer with `deny_unknown_fields` on all response types
- Optional remote adapter — disabled by default, explicit error if enabled in local registry
- Boundary layer: `strip_provider_fields` removes OpenAI-specific keys before Orchestrator sees them

**Known deviation:**
- Hardware-class detection not implemented; config drives everything via `models.yaml`

**Exit gates — all passed:**
- ✅ Identical SkyCore shape across local and remote adapter
- ✅ Missing model → explicit ModelNotFound error, no remote fallthrough
- ✅ Registry hot-reload without process restart
- ✅ Provider format never reaches Orchestrator/Agent Runtime/CLI (integration test: `strips_openai_fields_at_boundary`)

---

## Phase 4 — Persistent Coder Agent (Week 7–10)

**Goal:** End-to-end V1 product loop. Single agent, full pipeline.

**Orchestrator pipeline:**
```
classify task
  → load coder-primary identity          [orchestrator → runtime]
  → agent renders AgentIntent            [runtime]
  → resolve context_refs (mem + graph)   [orchestrator → memory/graph]
  → enforce context budget               [orchestrator]
  → secret redaction pass                [orchestrator]
  → trust enforcement                    [orchestrator]
  → build SkyCore request                [orchestrator]
  → invoke Inference Runtime             [orchestrator → inference]
  → receive normalized SkyCore response  [inference → orchestrator]
  → agent parses → DiffProposal          [orchestrator → runtime]
  → store in diff_proposals              [orchestrator → memory]
  → CLI shows diff; awaits approve       [orchestrator → CLI]
  → on approve: sign token               [orchestrator]
  → on apply: validate + apply + log     [orchestrator → tools]
```

**Built so far:**
- ✅ `coder-primary` identity loaded from `agents/` YAML files
- ✅ `agent_state` persisted and reloaded (SQLite `agent_state` table)
- ✅ Decision writer wired into `scos apply` — writes to `decisions` table on every apply
- ✅ `scos ask "<task>"` — full pipeline: classify → memory+graph context → model → diff → store; never auto-applies
- ✅ `scos approve <diff_id>` — ed25519 token, TTL=300s, single-use
- ✅ `scos apply <diff_id>` — validates token, applies via `git apply`, logs event, writes decision
- ✅ Editing strategy: new-file asks for `patch_unified`; edit-existing asks for `new_content` (full rewrite) and computes diff via `diffy` — eliminates model diff quality issues

**Phase 4 close — all items done:**
- ✅ `scos profile use <profile>` / `scos profile show` — reads/writes `agent_state` JSON
- ✅ `--profile` flag on `scos ask` — propagated through TaskLoopInput → SkyCore model_policy
- ✅ `model_invoked` events carry `profile_name` — `record_model_selection` wired into task_loop
- ✅ Decision recall across sessions — apply writes a `memories` row (scope='project') in addition to the `decisions` record; future tasks retrieve it via FTS5

**Exit gates — all passed (5/5 phase4_gate tests green):**
- ✅ Agent recalls a decision from session 1 in session 3 (`test_decision_recall_across_connections`)
- ✅ End-to-end offline edit working (new file + existing-file rewrite strategy both verified)
- ✅ All edits: diff → approval → apply → log; 0 unapproved writes
- ✅ Context tokens ≤ 50% naive baseline (graph+memory retrieval instead of full-file dumps)
- ✅ Exactly one agent in runtime (`test_exactly_one_agent_assertion`)
- ✅ `model_invoked` events carry `profile_name` (`test_model_invoked_event_carries_profile_name`)

---

## Phase 5 — Hardening + Router + Testing Lab (Week 10–12) ✅ CLOSED

**Goal:** Ship V1.

**Built:**
- 7 failure-mode tests (`runtime/tests/phase5_failure.rs`): expired token, patch conflict, SQLite busy, invalid YAML, migrate idempotent, FTS5 missing triggers, replay attack
- 11-test regression suite (`runtime/tests/phase5_regression.rs`): all prior phase gates re-verified in a single binary
- 3 router tests (`runtime/tests/phase5_router.rs`): 10-sample classifier (10/10), fallback fires, explicit failure on no models
- `TaskClass` updated with `PartialEq + Eq` derives for test assertions
- `ApplyError::GitApplyFailed` — patch conflict writes `diff_apply_failed` event, file untouched
- `memory/migrations/002_tuning_runs.sql` — idempotent `CREATE TABLE IF NOT EXISTS tuning_runs`
- Testing Lab: `scos profile bench`, `compare`, `tune`, `export-results` — all functional; `tuning_runs` populated on every run
- `docs/cli-reference.md` — all 16 commands documented with args and exit codes
- `deny.toml` — license allow-list + advisory policy at workspace root
- Workspace restructure: `skycode-core`, `skycode-tools`, `skycode-memory`, `skycode-graph`, `skycode-inference`, `skycode-agent`, `skycode-orchestrator` added as workspace members

**Known deviation:**
- Crate restructure is workspace-facade only: canonical crates exist as workspace members but implementation code remains in `runtime/` monolith. Actual module migration and `cargo deny` cross-dependency enforcement deferred to post-V1. Boundary is enforced by convention (same as Phase 0 deviation).
- `cargo deny check` not executed in CI (requires `cargo-deny` install); `deny.toml` policy is correct and ready.

**Exit gates — all passed:**
- ✅ Zero unapproved writes in full suite (`phase1_gate_50_edit_cycles_zero_unapproved_writes` + all failure tests)
- ✅ Offline demo: `phase2_gate_no_vector_db_or_remote_used` — remote adapter `enabled: false`; all 45 tests pass without network
- ✅ ≥50% context-token reduction (`test_graph_context_vs_naive_baseline`)
- ✅ Tool + decision logs reconstruct every applied change (traceability gate)
- ✅ SQLite p95 < 200ms read, p95 < 50ms write at 100k rows (`test_memory_retrieval_p95_under_200ms`)
- ✅ Router: 10/10 on hand-labelled samples; fallback fires on simulated primary failure; explicit error when no models
- ✅ `cargo build --workspace` + `cargo test --workspace` — 45 tests, 0 failures

---

## Phase 6 — Agentic Edit Loop + Hybrid Inference (Week 12–18) 🟡 OPEN

**Goal:** Transform from single-shot single-file diff generator into a feedback-driven
multi-file editor with parameterizable GPU/CPU/multi-GPU layer split. Close the
remaining V1 tech debt at the same time.

Phase 6 has four pillars. All four must close before the phase is marked CLOSED.

---

### Pillar 1 — Tech Debt Close

**Deliverables:**
- Move source code from `runtime/src/` into the canonical crates (`skycode-core`,
  `skycode-agent`, `skycode-tools`, `skycode-memory`, `skycode-graph`,
  `skycode-inference`, `skycode-orchestrator`). `runtime/` keeps a thin re-export
  shim only — already done for the workspace facade in Phase 5; this pillar
  completes the actual code migration.
- `cargo deny check` runs in CI and blocks license/advisory violations
- Deferred Phase 1 gates verified by named tests:
  - Red-team: no public write path other than `apply_diff`
  - UPDATE/DELETE triggers fire on `tool_events`, `approval_tokens_used`, `applied_changes`
  - Multi-file rollback on real repo

**Exit gates:**
- ⬜ `cargo build --workspace`: 0 warnings on canonical crates
- ⬜ `cargo deny check`: 0 violations
- ⬜ `phase6_redteam_no_extra_write_path`: workspace grep for `fs::write`,
  `fs::create_dir*`, `OpenOptions::*().write` returns zero results outside
  `skycode-tools::apply` and `#[cfg(test)]`
- ⬜ `phase6_append_only_triggers`: UPDATE/DELETE on protected tables raises
  constraint error
- ⬜ `phase6_multifile_rollback_real_repo`: 5-file edit + simulated mid-flight
  failure leaves repo identical to pre-apply state

---

### Pillar 2 — Multi-File Edits

**Deliverables:**
- New type `DiffSet { task_id, set_id: Uuid, diffs: Vec<DiffProposal> }`
- Migration 004:
  - `diff_sets (set_id PK, task_id, created_at)`
  - `diff_set_members (set_id, diff_id, ord, PRIMARY KEY (set_id, ord))`
- `agent::intent`: prompt template accepts and returns multiple `artifact` entries
- `boundary::sanitize_artifacts`: whitelist preserves arrays of artifacts
  (extends the Phase 4 `new_content` fix to N artifacts)
- `scos approve <set_id>`: signs the entire DiffSet — one ApprovalToken bound
  to the set, not per-diff
- `scos apply <set_id>`: atomic — `git apply --check` precheck on every diff;
  applies all or none. On mid-flight failure (rare since precheck), `git stash`
  recovers pre-apply state.

**Exit gates:**
- ⬜ `phase6_multifile_apply`: 10 sample tasks each touching 2+ files apply
  cleanly with single approval
- ⬜ `phase6_multifile_atomic`: simulated apply failure on diff 3-of-5 leaves
  repo identical to pre-apply state
- ⬜ All 48 V1 tests still green (single-file path is the N=1 specialisation
  of the multi-file path)

---

### Pillar 3 — Test-Verify Hook

**Deliverables:**
- Migration 005: `agent_state.test_command TEXT NULL` column
- New `EventType::TestVerifyPassed`, `EventType::TestVerifyFailed`
- `scos apply --verify` runs the configured `test_command` after apply,
  captures exit code + truncated stderr summary, logs as event
- `--verify` does NOT auto-revert on failure. The user inspects + decides.
  Auto-retry is intentionally deferred to a future phase to preserve V1's
  single-shot determinism guarantee.
- CLI: `scos profile use` learns `--test-command "<cmd>"`

**Exit gates:**
- ⬜ `phase6_verify_pass`: passing test → `test_verify_passed` event with exit 0
- ⬜ `phase6_verify_fail`: failing test → `test_verify_failed` event with stderr
  captured (≤4 KB), file changes preserved on disk
- ⬜ `phase6_verify_missing_cmd`: missing `test_command` with `--verify` → explicit
  error, no silent skip

---

### Pillar 4 — Hybrid Inference (GPU / CPU / Multi-GPU)

**Goal:** Make `agents/models.yaml` the single, parameterizable control point
for how transformer layers and KV cache are distributed across available
hardware. Reference example: a 6 GB-VRAM machine running a 7B Q4 model with
spillover to CPU RAM.

**New fields per model entry in `models.yaml`:**

```yaml
- name: local-coder
  gpu_layers: 28           # int, or "auto" → derived from vram_budget_mb
  kv_offload: false        # false → KV cache stays in CPU RAM (frees VRAM for weights)
  tensor_split: []         # [] = single GPU; [0.43, 0.57] = 6 GB + 8 GB
  split_mode: layer        # layer | row | none
  vram_budget_mb: 5500     # leave 500 MB headroom on a 6 GB GPU; "auto" reads VRAM
```

**Deliverables:**
- `inference::hardware`:
  - NVIDIA: `nvidia-smi -q -x` → `Vec<GpuInfo { index, vram_mb, name }>`
  - Windows non-NVIDIA: DXGI adapter enumeration
  - Returns empty Vec on machines with no discrete GPU (CPU-only path stays valid)
- `inference::loader`:
  - When `gpu_layers: "auto"`, computes optimal split from .gguf header
    metadata (param count, layer count) + KV cache estimate, against
    `vram_budget_mb`
  - When `tensor_split` non-empty, validates `sum ∈ [0.99, 1.01]`; rejects YAML
    with `RegistryError::InvalidTensorSplit` otherwise
  - Maps fields to llama-server flags: `--n-gpu-layers`, `--tensor-split`,
    `--split-mode`, `--no-kv-offload`
- `scos model verify` reports the chosen layer split and detected hardware
- Schema migration: none — `models.yaml` is already the source of truth and
  `runtime: openai_compatible` adapter is unaffected

**Exit gates:**
- ⬜ `phase6_hardware_detect_nvidia`: on a machine with NVIDIA GPU, returns
  ≥1 entry with non-zero VRAM; on CPU-only machine, returns empty Vec
  without error
- ⬜ `phase6_auto_layer_split`: 7B Q4 model on synthetic 6 GB-VRAM input →
  `gpu_layers ∈ [25, 30]` (heuristic; exact number depends on model architecture)
- ⬜ `phase6_multi_gpu_yaml`: registry parses `tensor_split: [0.43, 0.57]`
  and emits matching CLI flags to llama-server
- ⬜ `phase6_invalid_tensor_split`: `tensor_split: [0.5, 0.6]` → explicit
  parse error, never reaches llama-server
- ⬜ `phase6_gpu_vs_cpu_bench`: on machines with NVIDIA GPU, GPU configuration
  ≥2× tokens/sec vs CPU baseline (`scos model bench` comparison)
- ⬜ All Phase 3–5 CPU-only gates still pass when `gpu_layers: 0`

---

### Stretch (lift in only if Pillars 1–4 finish ahead of schedule)

- **Streaming inference:** llama-server SSE endpoint → token stream to CLI;
  first-token latency <500 ms reported by `scos ask`
- **`.git/HEAD` watcher:** auto-trigger `scos scan` on branch change
  (deferred from Phase 2)

---

### Migrations introduced in Phase 6

- `004_diff_sets.sql` — `diff_sets`, `diff_set_members`
- `005_agent_test_command.sql` — `ALTER TABLE agent_state ADD COLUMN test_command TEXT`

Both follow the idempotent `CREATE … IF NOT EXISTS` / `ALTER … IF NOT EXISTS`
pattern from `001`–`003`. Migration runner is unchanged.

---

### Phase 6 universal exit gate

- All Pillar 1–4 exit gates green
- `cargo test --workspace` ≥ V1 baseline (48) + new Phase 6 tests, 0 failures
- No new `unwrap()` outside `#[cfg(test)]`
- No layer-boundary violations (`skycode-core` still has zero internal deps,
  forbidden crossings still raise compile errors)
- All edits across the 6-week phase: diff → approval → apply → log; 0 unapproved
  writes (re-verify the Phase 1 invariant on the multi-file path)

---

## Universal Phase Gate Checklist

All lines must be `pass` before a phase closes.

| Gate | Criterion |
|------|-----------|
| Safety | Zero unapproved writes in full phase audit |
| Persistence | All state survives clean restart |
| Traceability | Every event queryable by `task_id` and timestamp |
| Boundary | No layer boundary crossed outside `docs/architecture.md` allowed list |
| Quality | Phase-specific numeric threshold met (see phase gates above) |
| Trust | Untrusted-mode invariants hold for any non-trusted path used in tests |
| Privacy | Secret scanner runs; no unredacted secret reaches memory, prompt, or log |
| Pinning | `docs/PINS.yaml` matches actual toolchain/grammars/SQLite/migrations head |
