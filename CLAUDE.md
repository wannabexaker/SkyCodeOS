# SkyCodeOS — Claude Session Config

## Project Identity

SkyCodeOS is a local, offline, single-agent Rust coding assistant.
Stack: Rust + SQLite + FTS5 + llama.cpp + tree-sitter.
Status: Phase 0 (documentation freeze). No code built yet.

## Skills

- `/init` — load session context (run this first in every session)
- `/skill-creator copilot phase-N` — generate Copilot prompt for phase N
- `/skill-creator codex phase-N` — generate Codex prompt for phase N
- `/skill-creator copilot review` — generate code review prompt
- `/graphify` — update knowledge graph after code changes
- `/readme` — regenerate README from docs
- `/repo-hygiene` — pre-release hygiene check

## Canonical Doc Precedence

`docs/*.md` > `docs/Plans/ClaudePlanMaster.md` > historical plans in `docs/Plans/`

Never treat historical plans as authoritative. If a doc in `docs/` contradicts a plan, the doc wins.

## What Claude Does Here

- Architecture decisions and cross-file consistency
- Security review of approval token pipeline, append-only enforcement, sandbox policy
- Creating and updating `docs/*.md` canonical files
- Phase gate reviews (one session per phase close)
- Generating prompts for Copilot and Codex via `/skill-creator`

## What Claude Does NOT Do Here

- Bulk Rust implementation (→ Copilot or Codex)
- Boilerplate generation (→ Copilot)
- Test suite generation (→ Copilot or Codex)
- SQLite migration scripts (→ Copilot or Codex)

## Layer Rules (memorize, never violate)

```
Models
  ↓ [adapter only]
Inference Runtime  (llama.cpp + optional remote adapter)
  ↓ [SkyCore Protocol only]
Orchestrator
  ↓
Agent Runtime / Memory / Graph / Tools
  ↓
CLI
```

Forbidden crossings:
- Agent Runtime → Model (direct) — FORBIDDEN
- Write to filesystem without ApprovalToken — FORBIDDEN
- CLI receives provider-format response — FORBIDDEN
- UPDATE or DELETE on tool_events, approval_tokens_used, applied_changes — FORBIDDEN

## Non-Negotiables

1. No silent writes. Every file mutation requires a signed ApprovalToken (UUID, TTL=300s, single-use).
2. tool_events is append-only and content-addressable. No UPDATE, no DELETE, ever.
3. Local-first. All Phase 0–4 gates must pass with network disabled.
4. No full-file context dumps. Graph + memory retrieval only.
5. Agent never bypasses orchestrator for tools or model access.
6. SQLite only until retrieval quality measurably fails with a benchmark.
7. Single agent (coder-primary) before any multi-agent work.

## Token Budget Policy

- Run `/init` at session start — loads minimum necessary context
- Do not re-read docs already loaded in this session
- Use `/skill-creator` to generate Copilot/Codex prompts instead of explaining architecture in chat
- Phase gate reviews: one focused session, not an ongoing conversation

## Current Phase

Phase 0 — Canonical Freeze (documentation).
Docs completed: architecture.md, protocol.md, schemas.md
Docs pending: See docs/ROADMAP.md for full list.
