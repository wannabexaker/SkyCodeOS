# llama.cpp WebUI — Feature Inventory

Captured: 2026-05-14
Source: live UI at `http://127.0.0.1:8081/#/` running against
`Qwen2.5-Coder 7B Instruct Q4_K_M.gguf` via `llama-server.exe`.

The llama.cpp project ships a static SvelteKit WebUI bundled with `llama-server`.
It is served from the same port as the inference API. This document inventories
its capabilities so we know what comes "for free" from upstream and what
SkyCodeOS should mirror, extend, or deliberately omit.

SkyCodeOS does **not** ship a WebUI. The visual frontend role belongs to
SkaiRPG. This inventory exists so that SkaiRPG and SkyCodeOS configuration
schemas stay aligned with what `llama-server` already supports.

---

## Top-level layout

- Dark theme by default. Theme toggle: System / Light / Dark.
- Left sidebar (compact icons or expanded labels):
  - sidebar toggle
  - new chat
  - search
  - MCP servers
  - settings
- Sidebar shows recent conversations list (e.g. one item: `a yooo`).
- Main pane: large centered chat input with `+` (upload) on the left and
  send arrow on the right. A small badge shows the loaded model
  (`Qwen2.5-Coder 7B Instruct Q4_K_M.gguf`).
- All settings persist in browser `localStorage`. No server-side user state.

## Settings categories

The settings panel is split into the following routes:

| Category       | Route                              |
|----------------|------------------------------------|
| General        | `#/settings/chat/general`          |
| Display        | `#/settings/chat/display`          |
| Sampling       | `#/settings/chat/sampling`         |
| Penalties      | `#/settings/chat/penalties`        |
| Agentic        | `#/settings/chat/agentic`          |
| Tools          | `#/settings/chat/tools`            |
| Import/Export  | `#/settings/chat/import-export`    |
| Developer      | `#/settings/chat/developer`        |
| MCP servers    | `#/settings/mcp`                   |

### General

- Theme (System / Light / Dark)
- API Key input — used when `llama-server` is started with `--api-key`
- System Message textarea — initial instruction injected into every conversation
- "Show system message in conversations" checkbox (default on)
- Paste-long-text-to-file threshold input (default `2500` chars)

### Display

- Show message generation statistics (tokens/sec, count, duration) — default on
- Show thought progress — default on
- Show tool call progress — default off
- Keep stats visible after generation — default off
- Show microphone on empty input — default off
- Render user content as Markdown — default off
- Use full-height code blocks — default off
- Disable automatic scroll — default off
- Always show sidebar on desktop — default off
- Show raw model names — partial visibility, default off

### Sampling

| Field                       | Default placeholder | Purpose |
|-----------------------------|---------------------|---------|
| Temperature                 | `0.8`               | Randomness control |
| Dynamic temperature range   | `0`                 | Dynamic temperature addon, adjusts by token entropy |
| Dynamic temperature exponent| `1`                 | Smooths probability redistribution |
| Top K                       | `40`                | Keep top k tokens only |
| Top P                       | `0.95`              | Cumulative probability cutoff |
| Min P                       | `0.05`              | Min probability relative to top token |
| XTC probability             | `0`                 | Chance of cutting top tokens; 0 disables |
| XTC threshold               | `0.1`               | Probability threshold required for XTC |
| Typical P                   | (empty)             | Sort/limit by log-prob vs entropy |
| Max tokens                  | `-1`                | Max output tokens; `-1` = unlimited |

Sampler order (default):

```
penalties,dry,top_n_sigma,top_k,typ_p,top_p,min_p,xtc,temperature
```

### Penalties

| Field               | Default | Purpose |
|---------------------|---------|---------|
| Repeat last N       | `64`    | Window for repetition penalty |
| Repeat penalty      | `1`     | Repetition penalty multiplier |
| Presence penalty    | `0`     | Penalize tokens that already appeared |
| Frequency penalty   | `0`     | Penalize by frequency of occurrence |
| DRY multiplier      | `0`     | DRY sampling strength; 0 disables |
| DRY base            | `1.75`  | DRY base value |
| DRY allowed length  | `2`     | DRY allowed repeat length |
| DRY penalty last N  | `-1`    | DRY penalty window |

### Agentic

- Agentic turns: `10` — max tool-call cycles before stopping (analogous to
  SkyLoopGuard's `DEFAULT_MAX_TOOL_CALLS = 50`)
- Max lines per tool preview: `25` — only previews + final response persist
  after the loop completes

### Tools

- Empty in default configuration: `No tools available`
- Tools are configured here (not yet populated)

### Import / Export

- Export conversations as JSON (all messages, attachments, history)
- Import conversations from JSON (merges with existing)
- Delete all conversations (red destructive section, no undo)

### Developer

- Pre-fill KV cache after response — resubmit the conversation to keep KV
  cache hot for the next turn
- Disable reasoning content parsing — sends `reasoning_format=none`,
  returns thinking inline
- Exclude reasoning from context — strips thinking before sending; default
  keeps it via `reasoning_content`
- Enable raw output toggle — switch chat to plain text instead of Markdown
- Custom JSON textarea — arbitrary JSON params merged into the API call

### MCP servers

- Separate page from the main settings panel
- Default state: `No MCP Servers configured yet. Add one to enable agentic features.`
- Top-right `+ Add New Server` button
- llama.cpp WebUI acts as an MCP **client** here (it can consume MCP servers)
- SkyCodeOS plays the opposite role: it **is** an MCP server

---

## What SkyCodeOS should adopt

These are features that map cleanly onto SkyCodeOS's existing architecture.

| llama.cpp feature        | SkyCodeOS surface to extend             |
|--------------------------|-----------------------------------------|
| Top K, Top P, Min P      | `AgentProfile` sampling fields          |
| Repeat last N            | `AgentProfile` (already has repeat_penalty) |
| Presence / Frequency pen | `AgentProfile` sampling fields          |
| Dynamic temperature      | `AgentProfile` optional field           |
| DRY family               | `AgentProfile` optional field           |
| XTC family               | `AgentProfile` optional field           |
| Typical P                | `AgentProfile` optional field           |
| KV cache prefill         | `ModelLaunchOptions` / `serve` config   |
| Reasoning format control | future `agents/profiles.yaml` field     |
| Custom JSON override     | future `agents/profiles.yaml` field     |
| Agentic turn limit       | already covered by `SkyLoopGuard`       |
| Tool preview line cap    | already covered by tool_events truncation |
| MCP server list          | already implemented (we ARE the server) |

## What SkyCodeOS should NOT duplicate

These belong to a frontend (SkaiRPG), not the engine.

- Browser-side `localStorage` settings persistence
- Theme controls (light/dark)
- Conversation list / search / export / import UI
- Display toggles (markdown rendering, scroll behavior, sidebar visibility)
- System message textarea
- File-upload UX

SkyCodeOS exposes these as **data** via its API/MCP/SSE endpoints. SkaiRPG
will render the UI. The engine stays headless.

## Reference: llama-server flags backing the WebUI

The WebUI is a thin client over `llama-server`'s HTTP surface. The chat
completion request body accepts the same fields the WebUI exposes:

```
temperature, dynatemp_range, dynatemp_exponent,
top_k, top_p, min_p, typical_p,
xtc_probability, xtc_threshold,
repeat_last_n, repeat_penalty,
presence_penalty, frequency_penalty,
dry_multiplier, dry_base, dry_allowed_length, dry_penalty_last_n,
n_predict (= max_tokens), grammar, response_format
```

All of these can be forwarded from SkyCodeOS's `ChatCompletionRequest`
without any llama-server changes.
