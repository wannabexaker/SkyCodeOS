# SkyCodeOS — Architecture

## Why "Operating System"

SkyCodeOS is not an application. It is the **foundational AI runtime** that
other applications sit on top of — the same relationship an OS has with the
programs it hosts.

| OS concept | SkyCodeOS equivalent |
|---|---|
| Manages hardware resources | GPU/VRAM auto-detect, tensor split, layer offload |
| Stable system-call interface | OpenAI-compatible API + MCP Protocol |
| Process scheduler | Task queue + agent orchestrator |
| Security model | ApprovalToken pipeline — no write without a signed token |
| Always-on service | `scos serve` daemon; clients connect, disconnect freely |
| Hardware abstraction | Same API surface on CPU-only and 4× GPU machines |

Applications never know they are talking to a `.gguf` model on a local GPU.
They see a standard OpenAI endpoint or an MCP server.

---

## Ecosystem Map

```
┌─────────────────────────────────────────────────────────────────┐
│                        Applications                              │
│                                                                  │
│   Cursor / IDE       SkaiRPG UI          Skycode Orchestrator    │
│   (OpenAI SDK)       (API / MCP)         (multi-agent, WIP)      │
└──────────┬───────────────┬───────────────────────┬──────────────┘
           │               │                       │
     OpenAI API       MCP Protocol            OpenAI API
           │               │                       │
           └───────────────┴───────────────────────┘
                           │
                           ▼
┌─────────────────────────────────────────────────────────────────┐
│                  SkyCodeOS  (runs on user's local PC)            │
│                                                                  │
│   port 11434  OpenAI-compatible REST API                        │
│   port 11435  MCP SSE server (LAN / Tailscale)                  │
│   stdio       MCP stdio server (Claude Desktop, Cursor plugin)   │
│                                                                  │
│   ── internal layers (Phase 1–6) ──────────────────────────     │
│   GGUF inference via llama.cpp                                  │
│   Memory: FTS5 SQLite                                           │
│   Tools: apply_diff, verify, search, rollback                   │
│   ApprovalToken pipeline (ed25519, single-use, TTL 300 s)       │
│   GPU auto-detect: nvidia-smi + Windows DXGI                    │
│   Multi-GPU tensor split                                        │
└─────────────────────────────────────────────────────────────────┘
                           │
             Tailscale / LAN  (single user, N machines)
                           │
┌─────────────────────────────────────────────────────────────────┐
│                      Local Hardware                              │
│   GPU / CPU   │   model.gguf   │   skycode.db (SQLite)          │
└─────────────────────────────────────────────────────────────────┘
```

**Deployment model:** One user, multiple machines, connected via Tailscale or
LAN. SkyCodeOS runs on the machine with the GPU. All other machines (laptop,
second workstation, phone via browser) connect to it over the private network.
No cloud required. No data leaves the LAN.

**SkaiRPG backend options** (user selects in settings):
1. **Ollama** — plain local inference, no SkyCodeOS tooling
2. **Skycode Orchestrator** — multi-agent, cloud or local
3. **SkyCodeOS** — local GGUF + full tool pipeline + approval security

All three expose the same OpenAI-compatible surface, so SkaiRPG uses one
HTTP client for all three.

---

# Skycode V1 Layer Architecture

## Canonical Stack

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

## Agent Runtime (Closed Responsibility)

**What the agent runtime does:**
- Loads identity from `agents/<id>/core/{soul,heart,mind,doctrine}.yaml`
- Holds and persists task-scoped working state via `agent_state`
- Builds `AgentIntent { goal, constraints, requested_tools, output_contract, context_hints }`
- Renders prompt fragments from identity + intent + context **handed in by orchestrator**
- Parses normalized SkyCore responses into agent-level outputs (`DiffProposal`, `Answer`, `Plan`)

**Agent Runtime Forbidden APIs** (lint + crate-deny enforced):
- `std::fs` — no filesystem access
- `std::net` — no network access
- `std::process` — no spawning subprocesses
- `skycode-tools` — no direct tool access
- `skycode-inference` — no model access
- `skycode-memory`/`skycode-graph` write API — memory writes are read-only observation only

**Architectural constraint:**
`skycode-runtime` depends on `skycode-core` only. No tool, memory, graph, or inference crate may be imported.

## Allowed Boundary Crossings

1. `CLI → Orchestrator` — user commands
2. `Orchestrator → Agent Runtime` — identity load, intent build, response parse
3. `Orchestrator → Memory/Graph` — read for context; write decisions/memories/state
4. `Orchestrator → Tools` — tool execution under policy
5. `Orchestrator → SkyCore client → Inference Runtime` — model invocation only
6. `Inference Runtime → Models` — GGUF/local; remote adapter only when `enabled: true`
7. `Tools → filesystem/git` — reads free; writes only via `apply_diff(diff_id, token)`
8. `Graph scanner → filesystem` — read-only; SQLite-only writes to graph tables

## Forbidden Crossings (with Controls)

| Boundary | Why | Control |
|---|---|---|
| Agent → Inference | Orchestrator owns model invocation | no `skycode-inference` dep in `skycode-runtime`; arch test |
| Agent → Tools | Agent returns structured intent only | no `skycode-tools` dep; arch test |
| Agent → filesystem/network/process | Agent is stateless intent builder | lint bans `std::fs`, `std::net`, `std::process` in `skycode-runtime` |
| Agent → Memory/Graph write | Only orchestrator writes decisions | runtime consumes read-only `ContextProvider` trait; writer lives in orchestrator |
| CLI → Inference/Memory/Graph/Tools direct | Orchestrator is the only mediator | cli depends only on `skycode-orchestrator` |
| Tools write outside `apply_diff` | No hidden writes | write fns private; `apply_diff` is sole public mutator; token validation first |
| Provider format above SkyCore | Strict protocol boundary | integration test scans all values reaching Orchestrator/Agent/CLI for raw provider fields |
| Audit table mutation | Immutable audit spine | triggers abort `UPDATE`/`DELETE` on `tool_events`, `diff_proposals`, `applied_changes`, `approval_tokens_used` |
| Remote adapter on by default | Local-first principle | registry default `enabled: false`; offline CI test asserts no socket open |
| Untrusted project writes | Trust model enforcement | trust enforcer blocks writes/terminal/remote before policy runs |
| Secret-bearing context to model | Privacy enforcement | secret redactor runs before SkyCore request build; unredactable matches abort task |
| Profile widening policy | Profiles are tuning, not policy | profile loader rejects touch to policy/approval/tools/remote/secrets/audit |

---

## Architectural Tests

Every phase gate includes:

1. **Boundary crossing test** — no forbidden arc traversed in test suite
2. **Provider format test** — raw provider fields never reach Orchestrator/Agent/CLI layers
3. **Append-only test** — `tool_events`, `diff_proposals`, `applied_changes`, `approval_tokens_used` never mutate
4. **Permission test** — Agent runtime cannot import tool/memory/graph/inference crates
5. **Process test** — No uncontrolled subprocess spawning outside tools layer
6. **Network test** — No socket operations outside Inference Runtime remote adapter; adapter disabled by default and in offline gates

