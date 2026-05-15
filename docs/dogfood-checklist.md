# Dogfood Checklist

Use this when testing SkyCodeOS for real coding work over a 1-2 day window.
Goal: surface bugs that the 132-test suite cannot catch — friction,
unexpected outputs, integration edge cases, UX papercuts.

Mark each item as you go. Bugs go in `docs/dogfood-findings.md` (create
on demand, gitignored if it contains sensitive paths).

## Session start

- [ ] `start-skycode.ps1` boots cleanly the first time today
- [ ] `scos profile show` returns the expected active profile
- [ ] Active profile makes sense for the work you plan to do
  (precise / fast / creative / deep / readonly / sandbox / experimental)
- [ ] Claude Desktop / Cursor lists all 8 `skycodeos.*` tools
- [ ] One MCP read-only call succeeds without prompting for credentials

## Chat-only paths (no repo writes)

Use these on a project you don't mind testing against. Read-only mode
recommended (`scos profile use readonly`).

- [ ] `search_memory` returns useful results for a real query you'd
      normally `Ctrl+F` for
- [ ] `get_diff` for a known diff_id returns the full patch in <2s
- [ ] Streaming via `/v1/chat/completions` from Cursor / Continue.dev
      produces fluent tokens with no gaps or truncation
- [ ] The agent stays inside its profile temperature (no surprise
      "creative" output on a `precise` profile)

## Real edit paths (writes allowed)

Switch back to `precise` or `fast`. Pick a real task you'd do anyway.

- [ ] `scos ask "<real task>"` produces a coherent diff on the first
      attempt
- [ ] The diff matches what you would have written by hand for at least
      80% of the change
- [ ] The `Approve? [y/N]` prompt is fast (<5s after diff is built)
- [ ] After `y`, the file appears on disk with the exact bytes from
      the diff
- [ ] `git status` shows the expected modification, nothing else
- [ ] `scos selfcheck` still passes after the change

## Edge cases worth provoking

- [ ] Ask for a change that requires `new file mode 100644` (creating a
      file). Phase 9 fix should handle it; verify.
- [ ] Ask for a multi-file change. `apply_diff_set` should keep the
      tree atomic — either all apply or none.
- [ ] Cancel mid-stream during a chat completion. The next call should
      work without restart.
- [ ] Kill `llama-server.exe` manually mid-task. `scos ask` should
      report a clean error, not hang.
- [ ] Two parallel `scos ask` invocations (different shells). Verify
      both finish and `tool_events` has both with distinct task_ids.
- [ ] Modify `agents/models.yaml` to point at a wrong path. Restart.
      The launcher should refuse with a clear error.

## MCP-specific

- [ ] Claude Desktop calling `run_verify` returns the actual test
      outcome (exit code, stdout/stderr summary)
- [ ] Claude Desktop calling `approve_diff` + `apply_diff` against a
      known `diff_id` writes the file
- [ ] Restart Claude Desktop after a SkyCodeOS rebuild — tools
      reappear within 5s of the first chat message
- [ ] Cursor agent uses `search_memory` to find context before
      proposing a change, not blindly

## Latency / load

- [ ] First chat completion of the day takes <2 min (model load)
- [ ] Subsequent completions take <30s for short prompts
- [ ] `/v1/events` SSE stream sends heartbeats while idle (keepalive
      every 15s) — open with `curl -N` and watch
- [ ] No memory bloat after 1 hour of mixed use
  (`Get-Process scos | Select-Object WorkingSet64`)

## What to write down

When something feels off, capture it now — not later. Each finding
should have:

- What you were trying to do
- What you expected
- What actually happened (verbatim error if any)
- A minimal reproduction if possible

Stash these in `docs/dogfood-findings.md`. After 1-2 days, group them
by category and decide the next phase from concrete friction, not
theoretical roadmaps.
