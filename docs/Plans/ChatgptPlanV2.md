# ChatGPT Plan v2: Skycode v1

## 1. V1 Definition

Skycode v1 is:

```text
one persistent coder agent
+ safe file editing
+ SQLite memory
+ graph-aware retrieval
+ local-first inference
+ CLI
```

Nothing else is v1.

Canonical layer order:

```text
Models
-> Inference Runtime
-> SkyCore Protocol
-> Agent Runtime
-> Memory + Graph + Tools
-> Orchestrator
-> CLI
```

No planned non-negotiable breaches.

## 2. V1 Audit

Wrong in my v1:

- Over-scoped the project as a multi-agent operating system for v1.
- Used "AI team" language too early; v1 is one coder agent.
- Included voice, UI, model marketplace behavior, relationship memory, and multi-agent phases in the main roadmap.
- Put multi-agent before a proven single-agent loop.
- Put model runtime too late relative to agent execution.
- Expanded `soul/heart/mind/doctrine` into personality mechanics. Docs require minimal stable fields only.
- Treated relationship memory as active v1 behavior. Docs allow only lightweight storage.
- Mentioned GraphRAG/vector search as future capability without hard gates. v1 is SQLite only.

Strong ideas taken from other plans:

- GitHubCopilotPlan: explicit non-negotiables, verification gates, and phase exit criteria.
- ClaudePlan: scope cuts, SQLite FTS5 first, tree-sitter over custom parsing, signed approval tokens, append-only tool log, UI post-v1.
- GeminiPlan: concise public framing around graph cognition, model agnosticism, and safe execution.
- DeepSeekPlan: schema discipline, graph node/edge vocabulary, and agent definition structure, stripped of civilization/personality/swarm scope.

V1 positions I still defend:

- Strict layer separation stays. It matches `/docs/architecture.md`.
- SkyCore stays provider-agnostic. UI/CLI never see provider formats.
- SQLite first stays. No vector DB until measured retrieval failure.
- Safe tools come before agent autonomy.
- Graph-aware retrieval is core, but v1 graph is structural: files, imports, exports, symbols, dependencies.
- llama.cpp is the local runtime target. Do not build an inference engine.

## 3. Public Contracts

SkyCore request/response remains the only model-facing contract:

```json
{
  "task_id": "uuid",
  "agent_id": "coder-primary",
  "goal": "Refactor auth module",
  "context_refs": ["memory:decision-23", "file:src/auth.ts"],
  "tools_allowed": ["read_file", "search_project", "create_diff"],
  "model_policy": {
    "preferred": "local-coder",
    "fallback": "local-fallback"
  },
  "output_contract": "diff_with_explanation"
}
```

```json
{
  "task_id": "uuid",
  "status": "ok",
  "summary": "Auth logic extracted to service.",
  "artifacts": ["diff:patch-001"],
  "requires_approval": true
}
```

Minimal agent definition:

```yaml
# soul.yaml
id: coder-primary
name: Coder Primary
role: persistent_coder
core_values: [safety, correctness, locality]

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

SQLite v1 tables:

```sql
memories(id, project_id, agent_id, scope, content, importance, created_at, updated_at)
decisions(id, project_id, agent_id, summary, rationale, context_refs, created_at)
agent_state(agent_id, project_id, state_json, updated_at)
tool_events(id, task_id, tool_name, input_hash, output_hash, status, created_at)
relationships(id, agent_id, target_id, note, created_at) -- lightweight only
graph_nodes(id, project_id, kind, name, path, span_json, metadata_json)
graph_edges(id, project_id, from_id, to_id, kind, metadata_json)
```

CLI v1 surface:

```bash
skycode scan <project>
skycode ask "<task>"
skycode diff <task-id>
skycode approve <diff-id>
skycode apply <approval-token>
skycode rollback <change-id>
skycode memory search "<query>"
skycode graph impact <path-or-symbol>
```

## 4. Boundary Rules

Allowed boundary crossings only:

- CLI -> Orchestrator: user commands only.
- Orchestrator -> Memory/Graph/Tools: context retrieval and policy-controlled tools.
- Orchestrator -> Agent Runtime: task execution only.
- Agent Runtime -> SkyCore Protocol: structured requests only.
- SkyCore Protocol -> Inference Runtime: provider-neutral model calls.
- Inference Runtime -> Models: llama.cpp/local model execution.
- Tools -> filesystem/git: only through policy, diff, approval, apply, log.

Breach risks and controls:

- File mutation: only `apply_diff(approval_token)` may write.
- Remote model use: optional adapter only; disabled by default; local path must pass all v1 gates.
- Terminal execution: generic shell tool deferred. v1 allows only explicit safe tools such as `git_status`.
- UI: deferred. CLI is the only v1 interface.
- Multi-agent: deferred. `coder-primary` is the only v1 agent.

## 5. Phased Plan

### Phase 0: Canonical Freeze

Implement no product behavior yet.

Deliverables:

- `/docs` remains source of truth.
- This plan becomes subordinate to `/docs`.
- Define SkyCore v0 structs, tool contracts, approval token contract, and SQLite migrations.
- Mark UI, voice, swarm, reviewer, manager, vector DB, emotional drift as post-v1 backlog.

Exit gates:

- No contradiction with `/docs`.
- Every v1 feature maps to one canonical layer.
- Every write-capable operation has a named approval gate.
- No v1 task requires a remote API.

Boundary crossings:

- None at runtime. Design only.

### Phase 1: Safe Tool Spine

Build tools before agents.

Deliverables:

- `read_file`, `list_dir`, `search_project`, `git_status`.
- `create_diff`, `apply_diff`, `rollback`.
- Signed approval token with TTL and single-use semantics.
- Append-only `tool_events` log.
- CLI commands for diff, approval, apply, rollback.

Exit gates:

- 50 simulated edit attempts produce zero unapproved writes.
- Direct filesystem write paths outside `apply_diff` fail tests.
- Multi-file rollback works on a real git repo.
- Every tool call creates a queryable log entry.

Boundary crossings:

- CLI -> Orchestrator.
- Orchestrator -> Tools.
- Tools -> filesystem/git.
- Breach risk: file mutation. Controlled by diff -> approval -> apply -> log.

### Phase 2: SQLite Memory + Graph V1

Build retrieval substrate.

Deliverables:

- SQLite migrations for memory, decisions, agent state, tool events, lightweight relationships.
- FTS5-backed keyword retrieval ranked by keyword relevance, recency, importance, scope.
- Project scanner.
- Graph v1 nodes: file, folder, symbol, import, export.
- Graph v1 edges: contains, imports, exports, depends_on, tested_by when discoverable.
- `skycode graph impact <path-or-symbol>`.

Exit gates:

- Scan persists project index across restart.
- Memory retrieval returns scoped results for project and agent.
- Graph impact query returns affected files for at least three real repo changes.
- No vector DB, embeddings, or remote service required.

Boundary crossings:

- Orchestrator -> Memory/Graph.
- Graph scanner -> read-only filesystem.
- Breach risk: none if scanner remains read-only except SQLite index writes.

### Phase 3: Local Inference Runtime + SkyCore

Wire models behind the protocol.

Deliverables:

- llama.cpp GGUF local runtime.
- Streaming response support.
- Model registry: name, runtime, context, strengths, speed, cost.
- Local primary/local fallback policy.
- Optional OpenAI-compatible adapter isolated behind Inference Runtime, disabled by default.
- Provider output normalized to SkyCore response shape.

Exit gates:

- Local model completes a SkyCore request with no network.
- Provider-specific fields do not cross into Agent Runtime, Orchestrator, or CLI.
- Missing model fails with explicit reason.
- Optional remote adapter can be disabled without breaking v1 tests.

Boundary crossings:

- Agent Runtime -> SkyCore Protocol.
- SkyCore Protocol -> Inference Runtime.
- Inference Runtime -> Models.
- Breach risk: remote dependency. Controlled by local-first gate and disabled remote default.

### Phase 4: Persistent Single Coder Agent

Build the v1 product loop.

Deliverables:

- `coder-primary` with minimal `soul`, `heart`, `mind`, `doctrine`.
- Agent state persisted in SQLite.
- Orchestrator flow:

```text
classify task
-> retrieve memory + graph context
-> invoke coder-primary
-> request model through SkyCore
-> create diff
-> request approval
-> apply approved diff
-> verify
-> log tool events and decision
```

- CLI `skycode ask "<task>"` proposes changes but never applies silently.

Exit gates:

- Agent recalls a decision from session 1 in session 3 after process restart.
- Agent completes one real safe edit offline using local model.
- Every produced edit goes through diff -> approval -> apply -> log.
- Prompt context is built from memory/graph refs, not whole-repo dumps.
- No second agent exists in v1 runtime.

Boundary crossings:

- CLI -> Orchestrator.
- Orchestrator -> Agent Runtime.
- Orchestrator -> Memory/Graph/Tools.
- Agent Runtime -> SkyCore.
- Breach risk: agent bypassing tools. Controlled by no direct tool/provider access from agent code.

### Phase 5: V1 Hardening

Release only after safety, persistence, traceability, and retrieval gates pass.

Deliverables:

- End-to-end regression suite.
- Offline install/run path.
- Benchmark against naive file dump retrieval.
- Failure-mode tests: missing model, stale approval token, patch conflict, restart mid-task, rollback failure.
- Minimal operator docs for CLI usage and model setup.

Exit gates:

- Zero unapproved writes across full test suite.
- Local-first demo works with network disabled.
- Context size reduced by at least 50% versus naive repo/file dumping on representative tasks.
- Tool and decision logs reconstruct every applied change.
- SQLite remains sufficient under measured v1 workloads.

Boundary crossings:

- All runtime boundaries exercised.
- Breach risk: release with hidden dependency or unsafe write path. Release blocked if found.

## 6. Test Plan

Required tests:

- Unit: approval token creation, expiry, single-use validation.
- Unit: tool policy rejects write attempts without approval.
- Unit: SkyCore request/response serialization.
- Unit: memory ranking by keyword, recency, importance, scope.
- Unit: graph node/edge creation from sample projects.
- Integration: diff -> approval -> apply -> rollback.
- Integration: scan -> graph impact -> context build.
- Integration: CLI ask -> diff generated -> no apply before approval.
- Integration: restart -> agent recalls decision and state.
- Offline: all v1 flows pass with network disabled.
- Regression: provider-specific model response never reaches CLI.

## 7. Post-V1 Backlog

Only after all v1 gates pass:

- Reviewer agent.
- Manager/architect roles.
- Tauri UI.
- Remote model fallback enabled by user config.
- Embeddings or vector DB, only after SQLite FTS5 and graph retrieval fail measured quality targets.
- Voice, multimodal, swarm, consensus, personality drift, relationship scoring.

Phase E from `/docs/roadmap.md` is v1.5/post-v1, not v1 acceptance.
