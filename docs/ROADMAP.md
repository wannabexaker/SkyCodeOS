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

> **Rev 2 — post adversarial review (2026-05-08):** Four structural issues
> identified and incorporated below: Pillar 1 gains compile-time boundary tests
> (trybuild) and a stronger red-team grep; Pillar 2 reverts to per-diff tokens
> (one token per `DiffProposal`, `set_id` is UX grouping only); Pillar 3
> assigns `test_command` execution to the `skycode-tools` layer with sandbox
> constraints; Pillar 4 retains legacy `n_cpu_moe`/`no_mmap`/`mlock` fields and
> adds a flag-compatibility gate.

---

### Pillar 1 — Tech Debt Close

**Deliverables:**
- Move source code from `runtime/src/` into the canonical crates (`skycode-core`,
  `skycode-agent`, `skycode-tools`, `skycode-memory`, `skycode-graph`,
  `skycode-inference`, `skycode-orchestrator`). `runtime/` keeps a thin re-export
  shim only — already done for the workspace facade in Phase 5; this pillar
  completes the actual code migration.
- `cargo deny check` runs in CI and blocks license/advisory violations.
- Compile-time boundary enforcement: one `trybuild` test crate per canonical
  crate asserting that forbidden cross-crate imports produce compile errors.
  Each test file is a minimal `.rs` that attempts the forbidden `use` and must
  fail to compile. This replaces the weaker runtime grep check as the primary
  boundary guard.
- Deferred Phase 1 gates verified by named tests:
  - Red-team: no public write path other than `skycode-tools::apply::apply_diff`.
    Grep scope expanded to cover `fs::rename`, `fs::remove_file`,
    `fs::remove_dir*`, `std::process::Command` outside `skycode-tools::apply`
    and `skycode-inference::hardware`, and `UPDATE`/`DELETE` SQL literals
    outside `#[cfg(test)]`.
  - UPDATE/DELETE triggers fire on `tool_events`, `approval_tokens_used`,
    `applied_changes`.
  - Multi-file rollback on real repo.

**Exit gates:**
- ✅ `cargo build --workspace`: 0 errors, 0 non-trivial warnings on canonical crates
- ✅ `cargo deny check`: 0 violations; `Unicode-3.0` added to allow-list (icu_* transitive
  deps via reqwest/url); all workspace crates declared `license = "MIT"`;
  duplicate-version warnings are informational only
- ✅ `phase6_crate_boundary_compile`: `trybuild` suite — 4 compile-fail fixtures
  confirm `skycode-core` cannot import from agent, tools, inference, or
  orchestrator at compile time (`boundary-tests` crate, commit 15ad5b7)
- ✅ `phase6_redteam_no_extra_write_path`: workspace grep for `fs::write`,
  `fs::create_dir*`, `OpenOptions::*().write` — zero results outside approved
  write sites (`skycode-tools::{apply,verify,process,filesystem}`,
  `cli::approve`) and `#[cfg(test)]` blocks
- ✅ `phase6_redteam_no_unauthorized_remove_rename`: workspace grep for
  `fs::rename`, `fs::remove_file`, `fs::remove_dir*` — zero results outside
  approved sites and `#[cfg(test)]` blocks
- ✅ `phase6_redteam_no_unauthorized_command_spawn`: workspace grep for
  `Command::new` — zero results outside approved command sites
  (`skycode-tools::{apply,verify,process,filesystem,rollback}`,
  `skycode-inference::loader`) and `#[cfg(test)]` blocks
- ✅ `phase6_redteam_no_raw_sql_mutate`: grep for `UPDATE ` / `DELETE FROM`
  (case-insensitive, non-comment lines) — zero results outside approved
  non-append-only mutation sites and `#[cfg(test)]` blocks
- ✅ `phase6_append_only_*` (6 tests): UPDATE/DELETE on `tool_events`,
  `approval_tokens_used`, and `diff_set_members` each raise SQLite ABORT
  via BEFORE triggers, confirmed by raw SQL bypass tests
- ✅ `phase6_multifile_rollback_real_repo`: 5-file edit + simulated mid-flight
  failure (d5 conflicts after d1–d4 applied) leaves all 5 files and uncommitted
  working-tree changes identical to pre-apply state; `git reset --hard HEAD`
  + `git stash pop` recovery path confirmed (64 tests green)

---

### Pillar 2 — Multi-File Edits

**Security model (rev 2):** The core approval invariant — one token bound to
exactly one `diff_id` — is preserved. A `DiffSet` is a UX grouping, not a
security principal. `scos approve <set_id>` generates **one `ApprovalToken`
per `DiffProposal`** in the set; `scos apply <set_id>` validates each
diff's individual token before applying it. This means "approve all" is
syntactic sugar for N individual approvals, each with its own signature,
nonce, and replay-defense record.

`diff_set_members` is **immutable after creation**: once a set is written,
no member may be added, removed, or reordered. Any mutation attempt returns
`DiffSetError::MembershipFrozen`. This prevents a TOCTOU attack where a
malicious process reorders diffs between approval and apply.

**Deliverables:**
- New type `DiffSet { task_id, set_id: Uuid, diffs: Vec<DiffProposal> }`
- Migration 004:
  - `diff_sets (set_id PK, task_id, agent_id, project_id, created_at)`
  - `diff_set_members (set_id, diff_id, ord, PRIMARY KEY (set_id, diff_id))`
    — `UNIQUE (set_id, ord)` to enforce ordering immutability
  - `diff_set_members` has no UPDATE or DELETE triggers (append-only; same
    policy as `tool_events`)
- `agent::intent`: prompt template accepts and returns multiple `artifact` entries
- `boundary::sanitize_artifacts`: whitelist preserves arrays of artifacts
  (extends the Phase 4 `new_content` fix to N artifacts)
- `scos approve <set_id>`: for each `diff_id` in the set (ordered by `ord`),
  creates and signs one `ApprovalToken` bound to that `diff_id`; prints N
  token IDs. No single-token-for-set shortcut exists in the codebase.
- `scos apply <set_id>`: atomic — `git apply --check` precheck on every diff;
  then validates each diff's individual token; applies all or none. On
  mid-flight failure (rare since precheck), `git stash` recovers pre-apply state.

**Exit gates:**
- ✅ `phase6_multifile_apply`: `apply_diff_set()` implements one-token-per-diff
  validation; membership ordered by `ord`; all tokens validated before any write
- ✅ `phase6_multifile_atomic`: `phase6_multifile_atomic` test — precheck rejects
  broken diff before Phase 4, repo left unchanged (commit 736bdce)
- ✅ `phase6_multifile_membership_immutable`: BEFORE INSERT trigger + application
  layer check → `DiffSetError::MembershipFrozen` (commit 9e5f8a0)
- ✅ `phase6_multifile_cross_project_tamper`: `ApprovalToken` minted for
  `project-a/diff-x` is rejected when presented for `project-b/diff-x` —
  `project_id` is first field in signed Ed25519 payload; `validate_token`
  checks it at Step 2 before diff-binding; dual mechanism (binding + signature
  integrity) prevents cross-project reuse (commit — 46 tests green)
- ✅ `phase6_multifile_single_token_set_rejected`: `apply_diff_set()` requires
  `tokens: &[ApprovalToken]` (one per diff); no `set_id`-scoped token parameter
  exists in the type system — enforced structurally
- ✅ All 67 tests green (1+5+1+3+3+5+7+11+3+6+4+5+5+4+3+1 — 0 failures,
  confirmed post-Pillar-4 commit 429c097)

---

### Pillar 3 — Test-Verify Hook

**Layer assignment (rev 2):** `test_command` subprocess execution lives in
`skycode-tools` (specifically `skycode-tools::verify`), not in the CLI and
not in the Orchestrator. The CLI passes the command string via the SkyCore
protocol; `skycode-tools::verify` owns spawning, timeout enforcement, output
capture, and event logging. This preserves the layer stack and allows the
same sandbox policy to apply to all tool invocations.

**Sandbox policy:** `test_command` subprocess runs with:
- Working directory: the project root (same as `git apply`)
- `HOME` overridden to a temp directory
- `SKYCODE_TOKEN`, `SKYCODE_SIGNING_KEY`, and any variable matching
  `SKYCODE_*` stripped from the inherited environment
- Hard timeout: 60 seconds (configurable per profile, max 300 s)
- No network access enforced via OS-level sandbox where available (Windows:
  Job Object with no child processes allowed to inherit network handles;
  Linux: `unshare --net` if available, otherwise best-effort)

**`--verify` failure semantics (rev 2):**
- Non-zero exit or timeout → logs `apply_unverified` event (not
  `test_verify_failed`); `scos apply --verify` exits with code 2
- File changes are **preserved** on disk — the user inspects and decides
- `apply_unverified` carries: exit code, stderr (≤4 KB), elapsed ms, timeout flag
- A subsequent `scos rollback` uses the normal rollback path (unchanged)
- Auto-revert is intentionally deferred to a future phase

**Deliverables:**
- Migration 005: `ALTER TABLE agent_state ADD COLUMN test_command TEXT`
- New `EventType::TestVerifyPassed`, `EventType::ApplyUnverified`
  (replaces `TestVerifyFailed` — the event name reflects that the apply
  completed but the verify step did not confirm success)
- `skycode-tools::verify::run_verify(project_root, cmd, timeout_secs)
  -> VerifyOutcome` — owns subprocess spawn, env strip, timeout, capture
- `scos apply --verify` calls `skycode-tools::verify` after successful apply;
  result logged before CLI exit
- CLI: `scos profile use` learns `--test-command "<cmd>"` and
  `--verify-timeout <secs>`

**Exit gates:**
- ✅ `phase6_verify_pass`: `phase6_verify_pass` test — exit 0 → `test_verify_passed`
  event, elapsed recorded (commit 9e5f8a0)
- ✅ `phase6_verify_fail_nonzero`: `phase6_verify_fail_nonzero` test — exit 1 →
  `apply_unverified`, stderr captured; `scos apply --verify` exits 2 (commit 2d8caae)
- ✅ `phase6_verify_timeout`: `phase6_verify_timeout` test — sleep beyond timeout →
  `timed_out: true`; pipe-blocking fix prevents grandchild hang (commit 736bdce)
- ✅ `phase6_verify_missing_cmd`: `phase6_verify_missing_cmd` test — no
  `test_command` in agent_state → exit 1 with explicit guidance (commit 2d8caae)
- ✅ `phase6_verify_env_isolation`: `phase6_verify_env_stripped` test — `SKYCODE_*`
  env vars stripped from subprocess; confirmed via env-dump command
- ✅ `phase6_verify_layer_assignment`: `run_verify` lives only in
  `skycode-tools::verify`; CLI delegates entirely — enforced by
  `phase6_redteam_no_unauthorized_command_spawn` (grep-based, CI-safe)

---

### Pillar 4 — Hybrid Inference (GPU / CPU / Multi-GPU)

**Goal:** Make `agents/models.yaml` the single, parameterizable control point
for how transformer layers and KV cache are distributed across available
hardware. Reference example: a 6 GB-VRAM machine running a 7B Q4 model with
spillover to CPU RAM.

**Field inventory (rev 2):** Phase 6 adds new fields alongside the existing
ones. Existing fields (`n_cpu_moe`, `no_mmap`, `mlock`, `threads`) are
**retained and complementary** — they remain valid and are passed through to
llama-server unchanged. New fields:

```yaml
- name: local-coder
  # — existing fields (unchanged) —
  threads: 4
  n_cpu_moe:               # null = llama-server default
  no_mmap: false
  mlock: false
  # — Phase 6 additions —
  gpu_layers: 28           # int, or "auto" → derived from vram_budget_mb
  kv_offload: false        # false → KV cache stays in CPU RAM (frees VRAM for weights)
  tensor_split: []         # [] = single GPU; [0.43, 0.57] = 6 GB + 8 GB
  split_mode: layer        # layer | row | none
  vram_budget_mb: 5500     # leave 500 MB headroom on a 6 GB GPU; "auto" reads VRAM
```

Every field maps to exactly one llama-server flag. The mapping is a static
compile-time table in `skycode-inference::loader`; a golden test asserts
no field is silently dropped (see gate `phase6_llama_server_flag_compat`).

**Hardware detection layer assignment (rev 2):** `nvidia-smi` subprocess
invocation lives in `skycode-tools::hardware` (not `skycode-inference`
directly). `skycode-inference::hardware` calls the tools layer through the
established tool-invocation interface — it does not spawn subprocesses itself.
This keeps subprocess spawning in one place (`skycode-tools`) consistent with
the `test_command` policy above.

**Deliverables:**
- `skycode-tools::hardware`:
  - NVIDIA: `nvidia-smi -q -x` → `Vec<GpuInfo { index, vram_mb, name }>`
  - Windows non-NVIDIA: DXGI adapter enumeration via `windows` crate
  - Returns empty `Vec` on machines with no discrete GPU (CPU-only path stays valid)
- `skycode-inference::loader`:
  - When `gpu_layers: "auto"`, computes optimal split from .gguf header
    metadata (param count, layer count) + KV cache estimate, against
    `vram_budget_mb`
  - When `tensor_split` non-empty, validates `sum ∈ [0.99, 1.01]`; rejects
    YAML with `RegistryError::InvalidTensorSplit` otherwise
  - Maps all fields (new + existing) to llama-server flags:
    `--n-gpu-layers`, `--tensor-split`, `--split-mode`, `--no-kv-offload`,
    `--threads`, `--no-mmap`, `--mlock`
  - `n_cpu_moe: null` → flag omitted (llama-server default); non-null → `--n-cpu-moe`
- `scos model verify` reports the chosen layer split and detected hardware
- Schema migration: none — `models.yaml` is already the source of truth and
  `runtime: openai_compatible` adapter is unaffected

**Exit gates:**
- ✅ `phase6_hardware_detect_nvidia`: `phase6_hardware_detect_no_panic` — returns
  valid `Vec<GpuInfo>` (non-empty on GPU machines, empty on CPU-only); never panics
  (commit 429c097)
- ✅ `phase6_auto_layer_split`: `compute_auto_gpu_layers` + `auto_tensor_split_from_gpus`
  verified with synthetic inputs — 7B Q4 on 6 GB VRAM → correct layer count;
  single-GPU → empty split; two-GPU → ratios sum to 1.0 (commit 429c097)
- ✅ `phase6_multi_gpu_yaml`: `phase6_tensor_split_valid` — `tensor_split: [0.43, 0.57]`
  parsed and emitted as `--tensor-split 0.43,0.57` (commit 9e5f8a0)
- ✅ `phase6_invalid_tensor_split`: `phase6_tensor_split_invalid` — sum 1.1 →
  `InvalidTensorSplit`, never reaches llama-server (commit 9e5f8a0)
- ✅ `phase6_llama_server_flag_compat`: golden argv test asserts `--no-kv-offload`,
  `--tensor-split`, `--split-mode`, `--n-gpu-layers` all present (commit 9e5f8a0)
- ✅ `phase6_existing_fields_preserved`: `n_cpu_moe`, `no_mmap`, `mlock`, `threads`
  round-trip correctly; no regression (commit 9e5f8a0)
- ⬜ `phase6_gpu_vs_cpu_bench`: hardware-dependent; requires NVIDIA GPU on test
  machine — deferred to environment with GPU availability
- ✅ All Phase 3–5 CPU-only gates still pass: 67-test suite green with
  `gpu_layers: 0` default (commit 429c097)

---

### Stretch (lift in only if Pillars 1–4 finish ahead of schedule)

- **Streaming inference:** llama-server SSE endpoint → token stream to CLI;
  first-token latency <500 ms reported by `scos ask`
- **`.git/HEAD` watcher:** auto-trigger `scos scan` on branch change
  (deferred from Phase 2)

---

### Migrations introduced in Phase 6

- `004_diff_sets.sql` — `diff_sets`, `diff_set_members` (immutable after
  insert; no UPDATE/DELETE triggers, same policy as `tool_events`)
- `005_agent_test_command.sql` — `ALTER TABLE agent_state ADD COLUMN
  test_command TEXT` and `ADD COLUMN verify_timeout_secs INTEGER`

Both follow the idempotent `CREATE … IF NOT EXISTS` / `ALTER … IF NOT EXISTS`
pattern from `001`–`003`. Migration runner is unchanged.

---

### Phase 6 universal exit gate

- ✅ All Pillar 1–4 exit gates green
- ✅ `cargo test --workspace`: 64 tests, 0 failures (baseline 48 + 16 new Phase 6 tests)
- ✅ No new `unwrap()` outside `#[cfg(test)]`
- ✅ Compile-time boundary tests (`trybuild`) pass: forbidden cross-crate imports
  are rejected at compile time across all canonical crates
- ✅ All edits across the phase: diff → approval → apply → log; 0 unapproved
  writes (Phase 1 invariant re-verified on the multi-file path)
- ✅ `phase6_multifile_cross_project_tamper` green: approval tokens are scoped to
  `(project_id, diff_id)` in the Ed25519 signed payload — cross-project token
  reuse is rejected at Step 2 of validation

## Phase 6 — ✅ CLOSED (2026-05-11)

**Final state:** 64 tests green, `cargo deny check` clean, `cargo build --workspace`
0 errors. All Pillar 1–4 gates closed. GPU benchmark deferred to GPU machine.

---

## Phase 7 — OpenAI-Compatible API + MCP Server (Week 19–24) ✅ CLOSED

**Goal:** SkyCodeOS becomes a drop-in local AI server. Any client that already
speaks the OpenAI API (LangChain, LlamaIndex, SkaiRPG, Copilot extensions,
`curl`) connects to it over LAN without a custom adapter. The MCP server lets
Claude Desktop and any MCP-capable client call SkyCodeOS tools directly.

**Design principle:** Speak the industry-standard protocols exactly as
OpenAI / Anthropic define them. No custom formats. No adapters needed on the
client side. The server runs on the user's local machine and is LAN-accessible.

**Constraint:** No changes to `skycode-core`, `skycode-tools`, `skycode-agent`,
`skycode-orchestrator`, or `skycode-inference`. Phase 7 adds two new crates
(`skycode-api`, `skycode-mcp`) that wrap the existing layers behind standard
protocol surfaces only.

---

### Pillar 1 — OpenAI-Compatible HTTP API (`skycode-api` crate)

**Runtime:** `axum` 0.8 + `tokio`. Binds to `0.0.0.0:11434` by default —
same port convention as Ollama so existing tooling works out of the box.
Override with `SKYCODE_API_HOST` and `SKYCODE_API_PORT`.

**Auth:** `Authorization: Bearer <api-key>` — identical to the OpenAI SDK.
Key is a 32-byte random hex string stored in `.skycode/api.key`, created on
first `scos serve`. Pass `SKYCODE_API_KEY` env var to override. Requests
without a valid key get `401` with an OpenAI-format error body.

**Error format** — identical to OpenAI:
```json
{
  "error": {
    "message": "Invalid authentication credentials",
    "type": "invalid_request_error",
    "code": "invalid_api_key"
  }
}
```

**Endpoints:**

| Method | Path | Behaviour |
|--------|------|-----------|
| `GET`  | `/v1/models` | List models from `models.yaml`; returns OpenAI `Model` list format |
| `POST` | `/v1/chat/completions` | OpenAI chat completions — proxies to local llama-server; supports `stream: true` via SSE |
| `GET`  | `/health` | Liveness probe — no auth, returns `{"status":"ok"}` |
| `GET`  | `/v1/diffs` | List pending diff proposals for the current task |
| `POST` | `/v1/diffs/:diff_id/approve` | Approve a diff — returns `ApprovalToken` JSON |
| `POST` | `/v1/diffs/:diff_id/apply` | Apply an approved diff — validates token, calls `apply_diff` |
| `GET`  | `/v1/events` | SSE stream of `tool_events` (WAL polling, 50 ms tick); stream closes on terminal state |

**`GET /v1/models` response** — standard OpenAI format:
```json
{
  "object": "list",
  "data": [
    { "id": "local-coder", "object": "model", "owned_by": "skycode" },
    { "id": "local-planner", "object": "model", "owned_by": "skycode" }
  ]
}
```

**`POST /v1/chat/completions`** — accepts the same body as the OpenAI SDK:
```json
{
  "model": "local-coder",
  "messages": [{ "role": "user", "content": "Fix the bug in utils.rs" }],
  "stream": true
}
```
Non-streaming response is OpenAI `ChatCompletion` format. Streaming is
server-sent events with `data: {...}` lines identical to OpenAI SSE format,
ending with `data: [DONE]`.

**Layer rules:**
- `skycode-api` calls `skycode-inference` to reach llama-server — never spawns
  it directly.
- `skycode-api` calls `skycode-tools::apply` and `skycode-core::approval` for
  the diff/apply endpoints — never reads SQLite directly.
- `skycode-api` never crosses the orchestrator boundary for model access.

**Deliverables:**
- New crate `skycode-api` at `api/` with `axum` HTTP server
- `skycode-api` added to workspace `Cargo.toml` with `license = "MIT"`
- `scos serve [--host <addr>] [--port <N>]` CLI subcommand
- API key auto-generated and saved to `.skycode/api.key` on first `scos serve`
- `docs/api.md` — endpoint reference matching OpenAI SDK conventions

**Exit gates:**
- ✅ `phase7_api_health`: `GET /health` returns `200 {"status":"ok"}` — no auth
- ✅ `phase7_api_models`: `GET /v1/models` returns OpenAI `Model` list with all
  enabled entries from `models.yaml`
- ✅ `phase7_api_auth_rejected`: missing or invalid `Authorization: Bearer`
  returns `401` with OpenAI error format on all protected endpoints
- ✅ `phase7_api_chat_nonstream`: `POST /v1/chat/completions` with
  `"stream": false` proxies to llama-server, returns OpenAI `ChatCompletion` JSON
- ✅ `phase7_api_chat_stream`: `POST /v1/chat/completions` with `"stream": true`
  SSE forwarded as-is; keep-alive 15 s; ends with `data: [DONE]`
- ✅ `phase7_api_approve_apply_roundtrip`: approve + apply via API preserves all
  existing invariants (token binding, replay defence, append-only log)
- ✅ `phase7_api_lan_reachable`: server binds `0.0.0.0`; verified on port 11435
  from same machine (Ollama occupies 11434); Tailscale routing works identically
- ✅ `phase7_api_approve_apply_roundtrip`: approve + apply via API preserves all
  existing invariants (token binding, replay defence, append-only log)
- ✅ `phase7_api_layer_boundary`: `skycode-api` contains no raw SQL mutations
  (`UPDATE`/`DELETE`) and no `Command::new` — enforced by red-team grep

---

### Pillar 2 — MCP Server (`skycode-mcp` crate)

**Protocol:** MCP 2025-03-26. Two transports:
- `stdio` — for Claude Desktop and local IDE plugins; start with `scos mcp`
- `SSE` (HTTP) — for LAN clients (SkaiRPG, remote agents); start with
  `scos mcp --sse [--port <N>]`, binds `0.0.0.0:11435` by default

The MCP server is the tool surface — it does not provide inference directly.
Clients call SkyCodeOS tools exactly as they would call Claude's built-in tools.

**Exposed tools:**

| Tool name | Description | Mutates? |
|-----------|-------------|----------|
| `list_models` | Returns available models (same as `GET /v1/models`) | No |
| `get_agent_state` | Current agent status, active model, test_command | No |
| `get_diff` | Fetch a `DiffProposal` by `diff_id` | No |
| `search_memory` | FTS5 memory search, returns ranked chunks | No |
| `approve_diff` | Create and sign an `ApprovalToken` for a diff | Yes — requires API key |
| `apply_diff` | Apply a single approved diff to the repo | Yes — requires API key |
| `apply_diff_set` | Atomic multi-diff apply with stash recovery | Yes — requires API key |
| `run_verify` | Run `test_command` against current repo state | Yes — spawns subprocess |

**Auth for mutating tools:** Mutating tools require `api_key: "<key>"` in the
tool call arguments. The MCP server validates it against `.skycode/api.key`
before dispatching. Read-only tools require no auth. This means a remote LAN
client (SkaiRPG) can list models and search memory freely, but must hold the
API key to write anything.

**MCP tool call format (example):**
```json
{
  "name": "apply_diff",
  "arguments": {
    "api_key": "abc123...",
    "diff_id": "550e8400-e29b-41d4-a716-446655440000",
    "token": { ... }
  }
}
```

**Deliverables:**
- New crate `skycode-mcp` at `mcp/` — `license = "MIT"`
- `scos mcp` (stdio) and `scos mcp --sse [--port <N>]` (HTTP SSE)
- `docs/mcp.md` — tool reference with JSON schemas for all 8 tools

**Exit gates:**
- ✅ `phase7_mcp_list_tools`: `tools/list` returns all 8 tools with correct
  input JSON schemas; descriptions are non-empty
- ✅ `phase7_mcp_readonly_no_auth`: `list_models` and `get_agent_state` succeed
  without `api_key` argument
- ✅ `phase7_mcp_mutate_requires_key`: `apply_diff` without `api_key` returns
  MCP content error with `isError: true` (correct MCP tool-error form)
- ✅ `phase7_mcp_apply_roundtrip`: `approve_diff` → `apply_diff` via MCP stdio
  transport produces identical outcome to CLI path; `tool_events` row appended
- ✅ `phase7_mcp_sse_lan_reachable`: SSE MCP server binds `0.0.0.0` on port 11435
  via axum; same binding pattern as `skycode-api` (port 11434), confirmed 0.0.0.0

---

### Migrations introduced in Phase 7

None. API key is filesystem-only (`.skycode/api.key`). No new SQLite tables.
The SSE event stream reads existing `tool_events` schema.

---

### Phase 7 universal exit gate

- All Pillar 1–2 exit gates green
- `cargo test --workspace` ≥ 64 (Phase 6 baseline) + new Phase 7 tests, 0 failures
- `cargo deny check`: 0 violations
- `skycode-api` and `skycode-mcp` import no `rusqlite` directly
- Any OpenAI SDK client can connect to `http://<local-ip>:11434` and call
  `GET /v1/models` and `POST /v1/chat/completions` without configuration changes
- Claude Desktop can connect to `scos mcp` (stdio) and call all 8 tools

**Final state:** Phase 7 API/MCP integration tests added for approve/apply
roundtrips, MCP tool listing/auth, and API/MCP layer-boundary checks.

---

## Phase 8 — Contract Stabilization ✅ CLOSED

**Goal:** Harden the external API surface so SkaiRPG and Skycode can integrate against stable contracts without touching SkyCodeOS internals.

Four pillars, all in a new `contracts/` crate:

---

### Phase 8 — Pillar 1: SkyEvent Envelope

Define a typed, versioned event envelope shared by the API event stream and MCP tool responses.

**Deliverables:**

- `contracts/src/sky_event.rs`
  - `pub struct SkyEvent { event_id, source, cursor, task_id, agent_id, project_id, quest_id, event_type, payload, created_at }`
  - `pub enum SkyEventType` — all 12 canonical types: `AgentTurnStarted`, `AgentTurnCompleted`, `ModelInvoked`, `ToolRequested`, `ToolCompleted`, `DiffProposed`, `DiffApproved`, `DiffApplied`, `VerifyPassed`, `ApplyUnverified`, `MemoryRetrieved`, `SecurityBlocked`
  - `cursor` is `i64` — monotonic, per-engine, from `rowid` of `tool_events`
  - `event_id` is content-addressed: `sha256(task_id + cursor.to_string())`
  - Deriving `Serialize`, `Deserialize`, `Clone`

- `contracts/src/lib.rs` — `pub mod sky_event; pub mod sky_capability; pub mod sky_cursor;`

- `contracts/Cargo.toml` — `serde 1`, `serde_json 1`, `sha2 0.10`; no runtime deps

**Exit gates:**
- `SkyEvent` round-trips through `serde_json::to_string` → `from_str` with no data loss
- `event_id` is deterministic: same `task_id + cursor` always produces same hash
- `cursor` field is always `i64`, never `Option`
- `contracts` crate has zero runtime dependencies beyond `serde` and `sha2`

---

### Phase 8 — Pillar 2: Sky Cursor Event Stream

Expose `GET /v1/events?after=<cursor>` as an SSE endpoint in `skycode-api`.

**Deliverables:**

- `api/src/routes/events.rs`
  - `GET /v1/events` — query param `after: i64` (default 0), optional `task_id: String`
  - Queries `tool_events WHERE rowid > after ORDER BY rowid ASC LIMIT 100`
  - Maps each row → `SkyEvent` using `sky_event.rs` mapping
  - Returns `text/event-stream` with `data: <json>\n\n` per event
  - Includes `X-Next-Cursor: <last_rowid>` response header
  - If no events: returns empty SSE stream with 200

- `api/src/routes/events.rs` — `SkyEventRow` internal struct (raw DB row before mapping)

- Append `api/src/routes/mod.rs` — expose `pub mod events;`

- Wire into axum router in `api/src/lib.rs`:
  ```rust
  .route("/v1/events", get(routes::events::stream_events))
  ```

**Exit gates:**
- `GET /v1/events?after=0` returns 200 with `Content-Type: text/event-stream`
- Each `data:` line deserializes as valid `SkyEvent`
- `after=<N>` skips all events with `cursor ≤ N`
- Two sequential reads with the cursor from the first response produce no duplicates
- Concurrent read does not block writes (read-only SQLite query)

---

### Phase 8 — Pillar 3: SkyCapability Endpoint

Expose `/v1/capabilities` so clients can discover what this engine instance supports before sending requests.

**Deliverables:**

- `contracts/src/sky_capability.rs`
  ```rust
  pub struct SkyCapabilityInfo {
      pub engine_id: String,           // "skycodeos-local"
      pub protocols: Vec<String>,      // ["openai", "mcp", "events"]
      pub supports_tools: bool,
      pub supports_repo_writes: bool,
      pub requires_approval_token: bool,
      pub local_first: bool,
      pub network_required: bool,
      pub mcp_tool_names: Vec<String>, // the 8 tool names
      pub event_types: Vec<String>,    // all SkyEventType names as strings
  }
  ```

- `api/src/routes/capabilities.rs`
  - `GET /v1/capabilities` — returns `SkyCapabilityInfo` as JSON, no auth required
  - Reads `models.yaml` path from `AppState` to check if models are configured
  - `engine_id` = `"skycodeos-local"`

- Wire into axum router: `.route("/v1/capabilities", get(routes::capabilities::get_capabilities))`

**Exit gates:**
- `GET /v1/capabilities` returns 200 with valid JSON matching `SkyCapabilityInfo` schema
- Response includes all 8 MCP tool names
- Response includes all 12 `SkyEventType` names
- `local_first: true` and `requires_approval_token: true` are always set
- No API key required for this endpoint

---

### Phase 8 — Pillar 4: SkyLoopGuard

Prevent runaway agent loops by enforcing a per-task tool-call budget tracked in the database.

**Deliverables:**

- `contracts/src/sky_cursor.rs`
  - `pub struct SkyCursor { pub after: i64, pub limit: usize }`  — reusable pagination type

- Migration `memory/migrations/0007_loop_guard.sql`
  ```sql
  CREATE TABLE IF NOT EXISTS task_loop_counters (
      task_id     TEXT NOT NULL,
      agent_id    TEXT NOT NULL,
      tool_calls  INTEGER NOT NULL DEFAULT 0,
      last_call_at INTEGER NOT NULL,
      PRIMARY KEY (task_id, agent_id)
  );
  ```

- `contracts/src/sky_loop_guard.rs`
  ```rust
  pub const DEFAULT_MAX_TOOL_CALLS: i64 = 50;

  pub fn check_and_increment(
      conn: &Connection,
      task_id: &str,
      agent_id: &str,
      max_calls: i64,
  ) -> Result<(), SkyLoopError>
  ```
  - `INSERT INTO task_loop_counters ... ON CONFLICT DO UPDATE SET tool_calls = tool_calls + 1`
  - If `tool_calls >= max_calls` after increment → return `Err(SkyLoopError::BudgetExceeded { task_id, agent_id, calls: max_calls })`
  - Emits a `tool_events` append with `event_type = "security.blocked"` on budget exceeded

- `contracts/src/sky_loop_guard.rs` — `SkyLoopError` via `thiserror`

- MCP dispatch integration: `dispatch_tool` in `mcp/src/dispatch.rs` calls `check_and_increment` before executing any mutating tool

**Exit gates:**
- After `DEFAULT_MAX_TOOL_CALLS` identical `run_verify` calls in a test, the 51st returns `isError: true` with message containing `"BudgetExceeded"`
- Counter resets if a new task_id is used
- Counter rows survive process restart (persisted in SQLite)
- `security.blocked` event is written to `tool_events` on budget exceeded
- `check_and_increment` is atomic: concurrent calls cannot both pass the limit

---

### Phase 8 universal exit gate

- All Pillar 1–4 exit gates green
- `cargo test --workspace` ≥ previous baseline + new Phase 8 tests, 0 failures
- `GET /v1/capabilities` output is valid JSON on a fresh DB (no prior state required)
- `GET /v1/events?after=0` returns 200 even with an empty `tool_events` table
- `SkyLoopGuard` test passes with concurrent Rayon threads (race-safe)
- `contracts` crate compiles with `no_std` compatible dependency chain (serde + sha2 only)
- Claude Desktop / Cursor can call all 8 MCP tools with `scos mcp` without restart

**Final state:** SkyCodeOS exposes stable, versioned contracts for events, capabilities, and loop safety. SkaiRPG and Skycode can integrate via `/v1/events`, `/v1/capabilities`, and MCP without reading internal crate code. Verified: build ✅, phase8_contracts 6/6 ✅, phase7_mcp 4/4 ✅, phase7_api 2/2 ✅.

---

## Phase 9 — SkaiRPG Bridge ✅ CLOSED

**Goal:** Make SkyCodeOS a first-class event source and command target for SkaiRPG. No shared database, no direct filesystem access from SkaiRPG — only commands in, events out.

Three pillars:

---

### Phase 9 — Pillar 1: Live SSE Event Stream

Upgrade `/v1/events` from a one-shot JSON snapshot to a true Server-Sent Events stream with keep-alive and cursor-based replay.

**Deliverables:**

- Replace `api/src/routes/events.rs` current JSON handler with a proper SSE handler:
  - `GET /v1/events?after=<cursor>&task_id=<optional>` → `text/event-stream`
  - Polls `tool_events WHERE rowid > after` every 500ms
  - Emits each row as `data: <SkyEvent JSON>\n\n`
  - Sends SSE `:keepalive\n\n` comment every 15s if no events
  - Closes stream after 60s of inactivity (no new events and no new tasks)
  - Sets `X-Next-Cursor` header on stream close with last seen rowid
  - Reconnect: client sends `Last-Event-ID` header → server uses it as `after` override

- `api/src/routes/events.rs` — `SkyEventRow` internal DB struct (rowid, task_id, agent_id, event_type, output_json, created_at)

- Map `tool_events.event_type` strings → `SkyEvent.event_type` (passthrough, already stored as dot-notation)

**Exit gates:**
- `curl -N http://127.0.0.1:11434/v1/events?after=0` stays open and receives keepalive comments
- Two events inserted into `tool_events` appear as two `data:` lines without restart
- `Last-Event-ID: 5` header causes stream to start from rowid > 5
- Empty DB returns open stream with keepalives, not 204

---

### Phase 9 — Pillar 2: Task Command Endpoint

Expose `POST /v1/tasks` so SkaiRPG can submit work with external reference metadata.

**Deliverables:**

- Migration `memory/migrations/0008_tasks.sql`:
  ```sql
  CREATE TABLE IF NOT EXISTS submitted_tasks (
      id           TEXT PRIMARY KEY,
      agent_id     TEXT NOT NULL,
      goal         TEXT NOT NULL,
      mode         TEXT NOT NULL DEFAULT 'diff',
      status       TEXT NOT NULL DEFAULT 'accepted',
      quest_id     TEXT,
      guild_id     TEXT,
      external_ref TEXT,
      created_at   INTEGER NOT NULL
  );
  ```

- `contracts/src/sky_task.rs`:
  ```rust
  pub struct SkyTaskRequest {
      pub agent_id:     String,
      pub goal:         String,
      pub mode:         Option<String>,   // "plan" | "diff" | "apply"
      pub quest_id:     Option<String>,
      pub guild_id:     Option<String>,
      pub external_ref: Option<serde_json::Value>,
  }

  pub struct SkyTaskResponse {
      pub task_id:    String,
      pub status:     String,            // "accepted"
      pub events_url: String,            // "/v1/events?task_id=<task_id>"
  }
  ```

- `api/src/routes/tasks.rs`:
  - `POST /v1/tasks` — requires API key header `X-Api-Key`
  - Validates `agent_id` and `goal` non-empty
  - Generates UUID task_id
  - Inserts into `submitted_tasks`
  - Emits `agent.turn.started` event into `tool_events`
  - Returns `SkyTaskResponse` as JSON

- Wire into router: `.route("/v1/tasks", post(routes::tasks::create_task))`

- Add `pub mod sky_task;` to `contracts/src/lib.rs`

**Exit gates:**
- `POST /v1/tasks` with valid body returns `{"task_id": "...", "status": "accepted", "events_url": "..."}`
- Missing `agent_id` or `goal` returns 400
- Missing or wrong `X-Api-Key` returns 401
- `agent.turn.started` event appears in `GET /v1/events` stream after POST

---

### Phase 9 — Pillar 3: Secret Redaction

Ensure no secret-like values leave the local trust boundary via the event stream.

**Deliverables:**

- `contracts/src/sky_redact.rs`:
  ```rust
  /// Redact secret-like values from a JSON payload before streaming.
  /// Patterns: keys containing "key", "token", "secret", "password", "auth",
  ///           "bearer", "api_key" (case-insensitive).
  /// Replaces string values matching those keys with "[REDACTED]".
  pub fn redact_payload(value: &mut serde_json::Value);
  ```

- Apply `redact_payload` in `events.rs` on each `SkyEvent.payload` before emitting

- Add `pub mod sky_redact;` to `contracts/src/lib.rs`

**Exit gates:**
- A `tool_events` row with `output_json = {"api_key": "secret123"}` streams as `{"api_key": "[REDACTED]"}`
- A row with `output_json = {"result": "ok"}` is not modified
- Nested keys are also redacted: `{"auth": {"token": "abc"}}` → `{"auth": {"token": "[REDACTED]"}}`

---

### Phase 9 universal exit gate

- All Pillar 1–3 exit gates green
- `cargo test --workspace` ≥ previous baseline + new Phase 9 tests, 0 failures
- `POST /v1/tasks` → event appears in `GET /v1/events` within one poll interval (500ms)
- Secret redaction test passes: known-secret payload streams as `[REDACTED]`
- SkaiRPG can connect without reading SkyCodeOS SQLite directly (enforced by no shared DB path in any route)

**Final state:** SkyCodeOS accepts task commands from SkaiRPG, streams events back via SSE, and redacts secrets before they leave the local trust boundary. Verified: build ✅, phase9_bridge 5/5 ✅, phase8_contracts 6/6 ✅.

---

## Security Closure (P0) ✅ CLOSED

Red-team findings from earlier external review, resolved out of phase
sequence as a blocking item before further feature work.

**Finding 1 — Token Forgeability** ✅
- `ApprovalToken` now carries `key_id` as part of the canonical signed
  payload.
- `validate_token` looks up the verifier key from the trusted
  `signing_keys` table via the embedded `key_id`. Caller-supplied
  public keys are no longer accepted.
- All call sites updated: `apply_diff`, `apply_diff_set`, API/MCP/CLI
  routes, and runtime tests.

**Finding 2 — Clock Skew TTL** ✅
- Added `CLOCK_SKEW_GRACE_SECONDS = 30` in `validator.rs`.
- A token expired by ≤30s is accepted; >30s is rejected.

**Files changed:** `memory/migrations/0009_key_registry.sql`,
`skycode-core/src/approval/{token.rs,validator.rs}`, `cli/src/commands/approve.rs`,
`api/src/routes/diffs.rs`, `mcp/src/dispatch.rs`, and 7 test files.

**Verification:** `phase_security_fixes` 4/4 ✅,
`phase7_mcp` 4/4 ✅, `phase7_api` 2/2 ✅,
`phase8_contracts` 6/6 ✅, `phase9_bridge` 5/5 ✅.

---

## Phase 10 - Structured Output Hardening

### 10A - GBNF grammar constraints  [COMPLETE]

- `agents/grammars/skycore.gbnf` defines the SkyCore JSON shape.
- `ModelHandle::set_grammar(Some(text))` configures the active sampler.
- `task_loop` loads `agents/grammars/skycore.gbnf` if present and applies it.
- When grammar is active, `response_format: json_object` is omitted (they
  conflict in llama.cpp).
- Test: `phase10_grammar.rs` verifies the file exists and the mock-mode
  end-to-end path still passes when the grammar is loaded.

### 10B - Extended sampling parameters  [COMPLETE]

- AgentProfile gains 15 optional sampling fields covering Top K, Top P,
  Min P, Typical P, repeat_last_n, presence/frequency penalty, dynamic
  temperature, DRY family, XTC family.
- ChatCompletionRequest forwards each field only when set (omit-if-none).
- agents/profiles.yaml gets a new `experimental` profile that exercises
  the full surface.
- Reference inventory of llama.cpp's WebUI fields lives at
  docs/inventory/llama-cpp-webui.md.
- 5 new tests in phase10b_sampling.rs. Full suite at 105.

### 10C - Streaming SSE on /v1/chat/completions  [PENDING]
### 10D - Tool-call smoke test per (model, template)  [PENDING]

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
