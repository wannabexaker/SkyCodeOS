# SkyCodeOS

Local, offline coding assistant. Runs a GGUF model on CPU, proposes file edits as diffs, and applies them only after a signed approval token.

No cloud. No Ollama. No internet required.

---

## How it works

```
skycode ask "<task>"
    │
    ├─ Loads agent identity (agents/coder-primary/core/*.yaml)
    ├─ Searches memory (SQLite FTS5) for relevant context
    ├─ Launches llama-server.exe (Qwen 7B Q4, CPU)
    ├─ Sends structured prompt → gets JSON response
    ├─ Parses diff from response
    ├─ Shows diff, asks y/N
    │
    ├─ [y] Signs ed25519 approval token (UUID, TTL=300s, single-use)
    │       → git apply writes the file
    │       → Event logged to SQLite (append-only)
    │
    └─ [N] Nothing written. Ever.
```

No file is ever written without a signed approval token. This is enforced at the database level — `tool_events` and `applied_changes` are append-only (SQLite triggers block UPDATE/DELETE).

---

## Current capabilities

### `skycode ask "<task>"`

Proposes a code edit for any task described in natural language.

```
cargo run -p skycode-cli -- ask "add error handling to src/lib.rs"
```

- Works on any file in a git repository
- Shows the unified diff before applying
- Requires explicit `y` approval
- Logs every event with a content-addressable ID

### Model: Qwen 2.5 Coder 7B (Q4_K_M)

- Runs fully on CPU via `llama-server.exe`
- First run takes 1–3 minutes to load the model
- Configured in `agents/models.yaml`
- Port 18080 (avoids Windows reserved ports)

### Memory (SQLite + FTS5)

- Stores context per project/agent/session scope
- BM25 ranking with recency decay
- Searched automatically on every `ask`

### Graph indexer (tree-sitter)

- Parses Rust, TypeScript, Python
- Incremental rescan on file change (mtime + size)
- `graph impact <symbol>` traces what a change affects

### Approval token pipeline

- ed25519 signature, UUID v4, TTL=300s
- Single-use enforced by atomic DB insert (replay detection)
- Token is bound to a specific diff ID and agent ID

---

## Setup

**Requirements:**
- Rust (stable)
- Git (in PATH)
- `llama-server` binary from [llama.cpp releases](https://github.com/ggml-org/llama.cpp/releases)
- A GGUF model — recommended: `Qwen2.5-Coder-7B-Instruct-Q4_K_M.gguf` from [HuggingFace](https://huggingface.co/bartowski/Qwen2.5-Coder-7B-Instruct-GGUF)

**Configure your model paths** in `agents/models.yaml`:

```yaml
executable: "/path/to/llama-server"   # your llama-server binary
path: "/path/to/model.gguf"           # your GGUF model file
port: 18080                            # local port (change if conflict)
```

**Run from the project root (must be a git repo):**

```
git init          # only needed once
cargo run -p skycode-cli -- ask "<your task>"
```

---

## Project structure

```
agents/
  models.yaml              # model registry (executable, path, port, ctx_size)
  coder-primary/core/      # agent identity (soul/heart/mind/doctrine YAMLs)

runtime/src/
  inference/               # llama-server launcher, health poll, HTTP chat completions
  orchestrator/            # task loop, router (classify → model), prompt builder
  agent/                   # identity loader, intent builder, state persistence
  memory/                  # store + FTS5 retrieval
  graph/                   # tree-sitter indexer, impact query
  approval/                # token sign + validate (ed25519, replay defense)
  tools/                   # diff create, git apply, rollback
  db/                      # migrations, append-only event logger

cli/src/commands/
  ask.rs                   # main command: propose → approve → apply
  model.rs                 # model verify/bench/load
  diff.rs / apply.rs       # lower-level plumbing

memory/migrations/
  001_initial.sql          # all 13 tables (memories, graph, diff_proposals, events, …)
```

---

## What's not wired yet

| Feature | Status |
|---|---|
| `skycode` global command | Old npm package is in PATH — use `cargo run` for now |
| Multi-session memory | DB persists but agent doesn't recall previous sessions yet |
| `skycode scan` | Graph indexer exists, CLI command not wired |
| `skycode rollback` | Logic exists, not integrated in ask flow |
| Remote/Ollama adapter | Disabled by design — local only |
