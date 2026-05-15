# Client Integration Guide

How to connect external tools to a running SkyCodeOS instance. Each section is
self-contained and includes a verification step. No client-side code needs to
change for SkyCodeOS to upgrade — the API and MCP contracts are stable.

## Prerequisites

Start SkyCodeOS once on your machine. Pick the surface you need:

```powershell
# Option A: OpenAI-compatible HTTP API on port 11434
.\start-skycode.ps1

# Option B: MCP stdio server (for Claude Desktop / Cursor MCP)
.\start-skycode.ps1 -Mcp

# Option C: MCP SSE server (for LAN clients on port 11435)
.\start-skycode.ps1 -Mcp -Sse
```

For Options B/C you usually do **not** run `start-skycode.ps1` directly — the
client launches the MCP server as a child process via the configured command.

Two facts every client integration depends on:

1. The API key lives at `.skycode/api.key` and is required for any HTTP
   request to `/v1/chat/completions`, `/v1/tasks`, `/v1/diffs`, or any
   mutating MCP tool (`approve_diff`, `apply_diff`, `apply_diff_set`,
   `run_verify`).
2. Local-model paths come from `agents/models.yaml`. The launcher reads
   them; clients only need the model `name` field.

## Claude Desktop (MCP stdio)

Config file:

```
%APPDATA%\Claude\claude_desktop_config.json   (Windows)
~/Library/Application Support/Claude/claude_desktop_config.json   (macOS)
~/.config/Claude/claude_desktop_config.json   (Linux)
```

Add `skycodeos` to `mcpServers`:

```json
{
  "mcpServers": {
    "skycodeos": {
      "command": "C:\\Projects\\SkyCodeOS\\scos-mcp.cmd",
      "args": [],
      "env": {
        "RUST_LOG": "warn",
        "SKYCODE_API_KEY": "<paste contents of .skycode/api.key>"
      }
    }
  }
}
```

On macOS / Linux replace the Windows wrapper with a shell launcher that
sets cwd and calls the binary:

```json
{
  "mcpServers": {
    "skycodeos": {
      "command": "/Users/me/code/SkyCodeOS/scos-mcp.sh",
      "args": [],
      "env": {
        "SKYCODE_API_KEY": "<paste contents of .skycode/api.key>"
      }
    }
  }
}
```

Restart Claude Desktop fully (quit from the tray, not just close the
window). The 8 SkyCodeOS tools appear under the tool list:

```
list_models       get_agent_state    get_diff       search_memory
approve_diff      apply_diff         apply_diff_set run_verify
```

Read-only tools work without auth. Mutating tools authorize via the
`SKYCODE_API_KEY` env var declared in the config above — Claude never
handles the credential in chat context.

Verification:

```
"List my SkyCodeOS models"
"Show my agent state"
"Run skycodeos.run_verify with timeout 30s"
```

## Cursor (MCP stdio)

Config file:

```
%USERPROFILE%\.cursor\mcp.json   (Windows)
~/.cursor/mcp.json   (macOS / Linux)
```

Same `skycodeos` block as Claude Desktop above. Restart Cursor. Tools
appear in the agent panel under the MCP server list.

**Cursor Free limitation.** The Cursor Free plan blocks `Agent` mode
from using named/custom models in the OpenAI-compatible models list —
you get a `Named models unavailable` banner. The MCP path is unaffected;
all 8 SkyCodeOS tools work on Free via MCP. Use the next section
(Continue.dev) if you need the chat completion path on Cursor Free.

## Continue.dev (VS Code extension)

Free alternative to Cursor that supports custom OpenAI-compatible
endpoints fully. Install the `Continue` extension from the marketplace,
then edit:

```
%USERPROFILE%\.continue\config.json   (Windows)
~/.continue/config.json               (macOS / Linux)
```

Add the model entry:

```json
{
  "models": [
    {
      "title": "SkyCodeOS local-coder",
      "provider": "openai",
      "model": "local-coder",
      "apiBase": "http://localhost:11434/v1",
      "apiKey": "<paste contents of .skycode/api.key>"
    }
  ],
  "tabAutocompleteModel": {
    "title": "SkyCodeOS fast",
    "provider": "openai",
    "model": "local-coder-fast",
    "apiBase": "http://localhost:11434/v1",
    "apiKey": "<paste contents of .skycode/api.key>"
  }
}
```

Reload the Continue panel. Streaming responses work because SkyCodeOS
forwards llama.cpp's SSE stream verbatim from
`/v1/chat/completions` (Phase 10C).

## Raw OpenAI SDK (Python)

```python
from openai import OpenAI

client = OpenAI(
    base_url="http://localhost:11434/v1",
    api_key="<paste contents of .skycode/api.key>",
)

resp = client.chat.completions.create(
    model="local-coder",
    messages=[{"role": "user", "content": "explain what GBNF grammar is"}],
    stream=True,
)
for chunk in resp:
    delta = chunk.choices[0].delta.content
    if delta:
        print(delta, end="", flush=True)
```

The same code works against any other OpenAI-compatible runtime — just
swap the `base_url`. SkyCodeOS adds zero proprietary fields to the
chat completion response (provider stripping at the SkyCore boundary).

## Raw OpenAI SDK (Node / TypeScript)

```typescript
import OpenAI from "openai";

const client = new OpenAI({
  baseURL: "http://localhost:11434/v1",
  apiKey: process.env.SKYCODE_API_KEY!,
});

const stream = await client.chat.completions.create({
  model: "local-coder",
  messages: [{ role: "user", content: "two-sentence summary of Phase 10C" }],
  stream: true,
});

for await (const chunk of stream) {
  process.stdout.write(chunk.choices[0]?.delta?.content ?? "");
}
```

## curl (raw HTTP)

```bash
# Health (no auth)
curl http://127.0.0.1:11434/health

# List models (auth)
curl -H "Authorization: Bearer $(cat .skycode/api.key)" \
     http://127.0.0.1:11434/v1/models

# Chat completion (auth) — streaming
curl -N \
     -H "Authorization: Bearer $(cat .skycode/api.key)" \
     -H "Content-Type: application/json" \
     -d '{
       "model": "local-coder",
       "stream": true,
       "messages": [{"role": "user", "content": "count to 3"}]
     }' \
     http://127.0.0.1:11434/v1/chat/completions

# Submit a structured task
curl -H "Authorization: Bearer $(cat .skycode/api.key)" \
     -H "Content-Type: application/json" \
     -d '{
       "agent_id": "coder-primary",
       "goal": "describe the architecture in one paragraph",
       "mode": "ask"
     }' \
     http://127.0.0.1:11434/v1/tasks

# Subscribe to the event stream
curl -N -H "Authorization: Bearer $(cat .skycode/api.key)" \
     http://127.0.0.1:11434/v1/events
```

## Endpoint reference

| Path | Method | Auth | Purpose |
|---|---|---|---|
| `/health` | GET | no | Health probe |
| `/v1/models` | GET | yes | Model list |
| `/v1/chat/completions` | POST | yes | OpenAI-compatible chat (supports `stream:true`) |
| `/v1/tasks` | POST | yes | Submit a SkyCore task |
| `/v1/diffs` | GET | yes | Recent diff proposals |
| `/v1/diffs/:diff_id/approve` | POST | yes | Sign an approval token |
| `/v1/diffs/:diff_id/apply` | POST | yes | Apply an approved diff |
| `/v1/capabilities` | GET | yes | Agent capabilities |
| `/v1/events` | GET | yes | Live SSE event stream |

All requests use `Authorization: Bearer <api_key>` from `.skycode/api.key`.

## MCP tool reference

8 tools exposed by `scos mcp` (stdio) and `scos mcp --sse` (HTTP):

| Tool | Auth | Purpose |
|---|---|---|
| `list_models` | no | Enabled models from `agents/models.yaml` |
| `get_agent_state` | no | Current agent_id, project_id, active_profile, test_command |
| `get_diff` | no | Fetch a `DiffProposal` by `diff_id` |
| `search_memory` | no | FTS5 memory search, BM25 ranked |
| `approve_diff` | yes | Sign and register an `ApprovalToken` |
| `apply_diff` | yes | Apply a single approved diff |
| `apply_diff_set` | yes | Atomic multi-diff apply with stash rollback |
| `run_verify` | yes | Run the configured `test_command` |

Authorized tools accept the key from either the call args (`"api_key": "..."`)
or the `SKYCODE_API_KEY` env var. The env var path is preferred for
chat-LLM clients so the credential stays out of conversation context.

## Troubleshooting

**MCP server appears, no tools show.** Restart the client fully (tray quit
on Claude Desktop, not just window close). Some clients cache the tool list
across reconnects.

**`Unauthorized: invalid api_key` on a mutating call.** Either the call
args sent the wrong key or the env var was missing/wrong. Verify the env
field in `claude_desktop_config.json` or `mcp.json` exactly matches
`.skycode/api.key`. The matched value is whitespace-sensitive — trim
trailing newlines when pasting.

**`database error: no such column ...`** This was fixed in commit
`47d8248`. If you still see it, rebuild and reinstall:
```
cargo install --path cli --force
```

**Cursor / Claude Desktop shows `MCP skycodeos: Server disconnected`.**
The MCP child process exited. Common cause: a rebuild of `scos.exe`
while the client held it open. Reload the MCP servers in the client UI,
or restart the client.

**HTTP 502 / `model backend unreachable`.** The `llama-server` sidecar
isn't running. Either the launcher could not start it (wrong path in
`agents/models.yaml`), or `scos serve` was started but the model failed
to load. Check the launcher output for the `Health check timed out`
error.

**Streaming response cuts off after a few chunks.** Pre-Phase-10C bug
in the SSE parser. Update to `cb22dc2` or later.

**Schema mismatch on `get_agent_state`.** Pre-`47d8248` bug. Update.
