# SkyCodeOS

Local Rust runtime for AI-driven file edits, gated by a signed approval token. Runs GGUF models offline via llama.cpp.

## Overview

Most AI coding assistants run in the cloud and expose a chat box. SkyCodeOS runs entirely on the user's machine and exposes three interfaces: an OpenAI-compatible HTTP API on `localhost:11434`, an MCP server on `:11435` (or stdio for Claude Desktop and Cursor), and a CLI named `scos`. No file is ever written without an `ApprovalToken` — a single-use ed25519 token bound to a specific diff, agent, and project, with a 300-second TTL.

The project is not an Ollama replacement. Ollama runs models. SkyCodeOS runs the full coding-agent loop: identity, memory, graph indexing, diff generation, approval pipeline, audit log, and tool orchestration.

## Features

- Local inference via `llama-server` (llama.cpp) as a supervised sidecar
- Six configurable profiles: `precise`, `fast`, `creative`, `deep`, `readonly`, `sandbox`
- Per-profile model, temperature, repeat penalty, max tokens, context budget, permission set
- ed25519 `ApprovalToken` with atomic single-use replay defense via SQLite `INSERT OR FAIL`
- Append-only audit: `tool_events`, `approval_tokens_used`, `applied_changes` are protected by SQLite `BEFORE UPDATE/DELETE` triggers
- 30-second clock-skew grace band on token TTL checks
- Key registry binds each token to a `key_id`; validators look up the public key from the database, not the caller
- FTS5 memory retrieval scoped per project/agent/session, BM25 with recency decay
- Tree-sitter graph indexer for Rust, TypeScript, Python; `graph impact <symbol>` traces transitive references
- OpenAI-compatible API endpoints: `/v1/chat/completions`, `/v1/models`, `/v1/diffs`, `/v1/tasks`, `/v1/events` (SSE)
- MCP 2025-03-26 protocol: stdio mode for Claude Desktop / Cursor, SSE mode for LAN clients
- Compile-time layer enforcement via `trybuild` compile-fail fixtures in `boundary-tests/`
- 98 integration tests covering append-only enforcement, approval pipeline, red-team write-path scan, security (replay, clock skew, key forgery), end-to-end anti-regression

## Architecture

Six layers, top to bottom: Models → Inference Runtime → SkyCore Protocol → Orchestrator → Agent Runtime / Memory / Graph / Tools → CLI. Each layer is a separate Rust crate. The `runtime/` crate is a compatibility shim that re-exports the underlying crates so the existing integration test suite keeps using a stable import path. Layer-crossing violations are enforced at compile time by the `boundary-tests/` crate.

### Components

| Component | Role |
|---|---|
| `skycode-core` | `ApprovalToken`, SkyCore request/response protocol, DB migrations |
| `skycode-tools` | `apply_diff`, `apply_diff_set`, `run_verify`, `rollback`, hardware detect |
| `skycode-memory` | FTS5 retrieval store, BM25 ranking, decision log |
| `skycode-graph` | Tree-sitter indexer, impact query |
| `skycode-inference` | `llama-server` lifecycle, model registry, chat completions |
| `skycode-agent` | Identity loader (soul/heart/mind/doctrine), `AgentProfile`, `PermissionSet` |
| `skycode-orchestrator` | Task loop, router, policy enforcement |
| `cli` | `scos` binary — subcommands: `ask`, `diff`, `approve`, `apply`, `scan`, `graph`, `model`, `profile`, `serve`, `mcp` |
| `api` | Axum HTTP server, OpenAI-compatible endpoints, SSE event stream |
| `mcp` | MCP 2025-03-26 server, stdio and SSE transports |
| `contracts` | `SkyEvent`, `SkyCursor`, `SkyCapabilityInfo`, `SkyLoopGuard` |

## Tech Stack

| Technology | Role |
|---|---|
| Rust 2021 | Implementation language |
| SQLite + FTS5 (rusqlite, bundled) | Memory store, audit log, key registry |
| llama.cpp (`llama-server`) | Local GGUF inference, OpenAI-compatible HTTP |
| tree-sitter | Code graph indexing (Rust, TypeScript, Python) |
| axum 0.8 | HTTP server for the OpenAI-compatible API and MCP SSE |
| ring 0.17 | ed25519 signing and verification |
| thiserror | Error types |
| clap 4 | CLI parsing |

## Installation

```bash
git clone https://github.com/wannabexaker/SkyCodeOS.git
cd SkyCodeOS
cargo install --path cli --force
```

Prerequisites:

- `llama-server` binary from [llama.cpp releases](https://github.com/ggml-org/llama.cpp/releases)
- A GGUF model — tested with `Qwen2.5-Coder-7B-Instruct-Q4_K_M.gguf` from [HuggingFace](https://huggingface.co/bartowski/Qwen2.5-Coder-7B-Instruct-GGUF)

Configure paths in `agents/models.yaml`:

```yaml
executable: "/path/to/llama-server"
path: "/path/to/model.gguf"
port: 18080
```

## Usage

The target directory must be a git repository for `scos ask` to apply diffs.

```bash
git init
scos ask "add error handling to src/lib.rs"
```

`scos ask` proposes a unified diff, prints it, and prompts `Approve? [y/N]`. On `y`, it signs an `ApprovalToken` and applies the diff via `git apply`. On `N`, nothing is written.

Configure the active profile:

```bash
scos profile list
scos profile use precise
scos profile use --temperature 0.7 --model local-coder
scos profile use readonly        # blocks all file writes
```

Start the OpenAI-compatible API (LAN-accessible):

```bash
scos serve --host 0.0.0.0 --port 11434
```

Start the MCP server:

```bash
scos mcp                              # stdio mode (Claude Desktop, Cursor)
scos mcp --sse --port 11435           # LAN SSE mode
```

## Project Structure

```
SkyCodeOS/
├── agents/
│   ├── models.yaml              — model registry (path, executable, port, ctx_size)
│   ├── profiles.yaml            — profile config (model, temperature, permissions)
│   └── coder-primary/core/      — agent identity (soul/heart/mind/doctrine)
├── skycode-core/                — ApprovalToken, SkyCore protocol, DB
├── skycode-tools/               — apply_diff, verify, rollback
├── skycode-memory/              — FTS5 retrieval
├── skycode-graph/               — tree-sitter indexer
├── skycode-inference/           — llama.cpp launcher
├── skycode-agent/               — identity, profiles
├── skycode-orchestrator/        — task loop, router, policy
├── runtime/                     — compat re-exports + integration tests
├── api/                         — Axum HTTP server (OpenAI-compatible)
├── mcp/                         — MCP server (stdio + SSE)
├── contracts/                   — SkyEvent, SkyCursor, SkyCapabilityInfo
├── cli/                         — scos binary
├── memory/migrations/           — SQLite schema (FTS5, append-only triggers)
├── docs/                        — architecture, protocol, schemas, roadmap
└── boundary-tests/              — compile-fail layer-crossing enforcement
```

## Notes

`tool_events`, `approval_tokens_used`, `applied_changes`, and `diff_set_members` are protected at the database level. Application code cannot bypass append-only enforcement — the SQLite `BEFORE UPDATE` and `BEFORE DELETE` triggers raise an error on any mutation attempt.

The signing key registry binds each `ApprovalToken` to a `key_id` recorded in the database at registration time. Validators look up the public key from the registry by `key_id`, not from the caller. A 30-second grace band on TTL checks tolerates NTP-driven clock drift.

The `mock_model_response.json` intercept in `task_loop.rs` is gated by file existence in `.skycode/`. In production the file does not exist, so the intercept is never reached. It is used by `runtime/tests/anti_regression_e2e.rs` to verify the full end-to-end chain (model JSON → diff → token → `git apply`) without needing a GPU or a real model.

The `runtime/` crate exists only as a re-export shim for the older monolithic API used by the integration tests. New code should import from the per-responsibility crates directly.

## Future Improvements

- Streaming SSE responses on `/v1/chat/completions` (currently returns a single JSON body)
- Sandbox profile actually writes to `.skycode/sandbox/` (currently logs a warning and falls through)
- Embeddings endpoint as an optional memory feature alongside FTS5
- `llama-bench` JSON import into `model_benchmarks` for routing decisions
- Align per-crate `Cargo.toml` license fields with the top-level Apache 2.0 `LICENSE`
