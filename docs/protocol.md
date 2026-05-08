# SkyCore Protocol — V1 Contract

## Overview

SkyCore is the normalized protocol between the orchestrator and the inference runtime. It defines request shape, response shape, and validation rules. No provider-specific formats (OpenAI `choices`, Anthropic `content_block`, llama.cpp raw tokens) ever cross the SkyCore boundary.

---

## SkyCore Request

```json
{
  "skycore_version": "0.1",
  "task_id": "uuid-v4",
  "agent_id": "coder-primary",
  "goal": "string describing the refactoring or analysis task",
  "context_refs": [
    "memory:<id>",
    "graph:<kind>:<id>",
    "file:<repo-relative-path>",
    "decision:<id>"
  ],
  "tools_allowed": [
    "read_file", "list_dir", "search_project",
    "git_status", "git_diff", "create_diff"
  ],
  "model_policy": {
    "preferred": "local-coder",
    "fallback": "local-fallback",
    "profile": "precise"
  },
  "output_contract": "diff_proposal",
  "constraints": {
    "max_output_tokens": 4096,
    "stream": true,
    "stop": []
  }
}
```

**Fields:**

| Field | Type | Required | Notes |
|---|---|---|---|
| `skycore_version` | string | yes | Always `"0.1"` in v1 |
| `task_id` | uuid | yes | Task identifier for tracing |
| `agent_id` | string | yes | Always `"coder-primary"` in v1 |
| `goal` | string | yes | User-facing refactoring/analysis prompt |
| `context_refs` | array | yes | References to memory, graph, files, decisions |
| `tools_allowed` | array | yes | Which tools agent may request |
| `model_policy.preferred` | string | yes | Primary model name from registry |
| `model_policy.fallback` | string | yes | Fallback model if primary fails |
| `model_policy.profile` | string | yes | Tuning profile name (resolved per Patch 21) |
| `output_contract` | enum | yes | One of: `"diff_proposal"`, `"answer"`, `"plan"` |
| `constraints.max_output_tokens` | int | yes | Enforce token limit in streaming |
| `constraints.stream` | bool | no | If `true`, streaming response expected |
| `constraints.stop` | array | no | Stop sequences for generation |

---

## SkyCore Response

```json
{
  "skycore_version": "0.1",
  "task_id": "uuid-v4",
  "status": "ok",
  "summary": "Extracted auth logic into AuthService; updated imports in 3 files.",
  "artifacts": [
    {
      "kind": "diff",
      "id": "patch-001",
      "patch_unified": "--- a/src/auth.rs\n+++ b/src/auth.rs\n@@ ...",
      "affected_files": ["src/auth.rs", "src/main.rs", "src/lib.rs"]
    },
    {
      "kind": "memory",
      "id": "mem-refactor-001"
    }
  ],
  "tool_calls_requested": [
    {
      "tool": "search_project",
      "inputs": {
        "query": "LoginHandler",
        "scope": "src/"
      }
    }
  ],
  "requires_approval": true,
  "error": null
}
```

**Fields:**

| Field | Type | Required | Notes |
|---|---|---|---|
| `skycore_version` | string | yes | Always `"0.1"` |
| `task_id` | uuid | yes | Echo back request `task_id` |
| `status` | enum | yes | One of: `"ok"`, `"error"`, `"needs_approval"`, `"needs_tool"` |
| `summary` | string | yes | Human-readable summary of work performed |
| `artifacts` | array | yes | Diffs, memory entries, analysis results |
| `tool_calls_requested` | array | yes | If agent is requesting tool execution |
| `requires_approval` | bool | yes | Whether orchestrator must gate before apply |
| `error` | object \| null | yes | If `status: "error"`, includes `{ code, message }` |

**Critical rule:** No raw provider fields allowed at this layer. Output from `choices[0].message.content` (OpenAI), `content[0].text` (Anthropic), or llama.cpp token stream is stripped and normalized before SkyCore response is built.

---

## Approval Token Contract

Tokens are signed, single-use, scoped, and time-bound. They defend against replay attacks and guarantee immutable diff binding.

### Token Format

```
token := base64url( payload || "." || ed25519_sig(payload) )

payload := {
  "tid":  "<uuid-v4>",           // token id, unique, single-use (PK in approval_tokens_used)
  "scp":  "apply_diff",          // scope; only this scope grants writes
  "did":  "<diff-id>",           // immutable diff_id binding
  "tsk":  "<task-id>",           // origin task
  "exp":  <unix-seconds>,        // now + 300 seconds
  "kid":  "<key-id>"             // signing key id, rotatable per key management policy
}
```

### Validation Order (Fail Fast)

1. Decode base64url; split payload and ed25519 signature.
2. Parse payload JSON; verify schema against struct.
3. Resolve signing key by `payload.kid`. Reject on unknown key.
4. Verify ed25519 signature over canonical payload bytes (deterministic JSON). Reject on signature mismatch.
5. Reject if `payload.exp <= now()` (token expired).
6. Reject if `payload.scp != "apply_diff"` (scope mismatch; only apply_diff writes allowed).
7. Reject if `payload.did` ≠ requested `diff_id` argument (diff binding).
8. Reject if `payload.tsk` ≠ origin task of the diff (task binding).
9. **Atomic INSERT** into `approval_tokens_used(tid, diff_id, task_id, used_at)`. Primary key violation → replay defense → reject.
10. Load immutable `diff_proposals` row by `payload.did`. Reject if missing, expired, or different project scope.
11. Verify `base_blob_hashes_json` against current working tree. Mismatch → emit `diff_apply_failed` event, exit code 4.
12. Apply patch onto isolated branch (§4.12 git isolation).
13. Append `diff_applied` event with `approval_token_id = payload.tid` and `diff_id = payload.did`.

**Key guarantee:** The token never carries diff bytes. The diff is loaded from the immutable `diff_proposals` table, protecting against token-injection attacks.

---

## Protocol Boundary Enforcement

### Inbound to Orchestrator

- All requests decoded and validated against SkyCore schema before use.
- Any unrecognized field triggers a schema mismatch error (fail-safe).
- `context_refs` are validated: memory ids exist, graph ids resolve, files are within repo bounds.

### Outbound to Agent Runtime

- Agent runtime receives only normalized context (memory snippets, graph metadata, file previews).
- No raw SkyCore request handed to agent; only extracted intent (goal, constraints, context hints).
- Agent runtime parses normalized SkyCore response and returns structured output.

### Outbound to Inference Runtime

- Orchestrator builds SkyCore request, validates all fields, encodes to JSON.
- Passes to inference runtime via standard in/stdout or local socket.
- Receives SkyCore response JSON; validates schema before parsing.
- All provider-specific fields stripped before response reaches Orchestrator business logic.

### Architectural Test

Integration test `test_provider_format_boundary()` scans every artifact at Orchestrator → Agent and Orchestrator → CLI boundaries for provider-specific fields (`choices`, `delta`, `content_block`, raw `usage`, raw token ids, role strings, etc.). Failure blocks PR merge.

