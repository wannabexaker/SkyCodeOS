# Security Policy

## Supported Versions

| Version | Supported |
|---------|-----------|
| `main` branch | ✅ |

## Reporting a Vulnerability

**Do not open a public GitHub issue for security vulnerabilities.**

Report security issues privately via GitHub's Security Advisories:
[https://github.com/YOUR_USER/SkyCodeOS/security/advisories/new](https://github.com/YOUR_USER/SkyCodeOS/security/advisories/new)

Include:
- Description of the vulnerability
- Steps to reproduce
- Potential impact

You will receive a response within 7 days.

## Security Model

SkyCodeOS is designed with a defense-in-depth approach:

- **No write without approval** — every file mutation requires a signed `ApprovalToken` (ed25519, UUID v4, TTL=300s, single-use).
- **Replay defense** — token IDs are stored in `approval_tokens_used` via atomic `INSERT OR FAIL`. Replayed tokens are rejected at the DB level.
- **Append-only audit** — `tool_events`, `approval_tokens_used`, and `applied_changes` have SQLite `BEFORE UPDATE/DELETE` triggers that raise errors. These tables can never be modified after write.
- **Key registry binding** — approval tokens carry a `key_id`; validators look up the public key from the DB registry, not from the caller.
- **Local-first** — no network calls in the default configuration. The inference runtime runs as a local subprocess.

## Out of Scope

- Issues in `llama.cpp` itself (report upstream at https://github.com/ggml-org/llama.cpp)
- Issues in GGUF model weights
- RPC/network exposure when `llama-server` is intentionally bound to a public interface by the user
