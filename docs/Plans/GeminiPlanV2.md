# Skycode Implementation Plan v2 (Gemini)

## 1. V1 Audit & Alignment

### 1.1 What was wrong in my V1[cite: 7]
*   **Over-scoped rhetoric:** Entertained the "AI Civilization" and "Digital Firm" metaphors. V1 is a single CLI tool[cite: 4, 5].
*   **Premature Multi-Agent:** Scheduled Reviewer and Security agents for V1[cite: 7]. V1 must be `coder-primary` only[cite: 4].
*   **Premature UI:** Recommended Tauri in V1[cite: 7]. V1 is CLI-only[cite: 4, 8].
*   **Cloud Reliance Risk:** Positioned "Big AI" as a core component[cite: 7]. V1 must be fully functional offline via `llama.cpp`[cite: 4, 8].

### 1.2 Ideas Stolen from Other Plans
*   **From ChatGPT (Source 4):** Strict layer separation enforcement and the explicit SkyCore JSON contract[cite: 4].
*   **From Claude (Source 5):** Approval tokens must be signed UUIDs with TTLs; tool logs must be append-only and content-addressable; use `tree-sitter` instead of custom AST parsing[cite: 5].
*   **From GitHub Copilot (Source 8):** Explicit verification gates (Safety, Persistence, Traceability, Quality) per phase[cite: 8].
*   **From DeepSeek (Source 6):** The base structural vocabulary for graph nodes and edges[cite: 6].

### 1.3 Defended V1 Positions
*   **The 4-File Agent Identity:** I retain the `soul/heart/mind/doctrine` split[cite: 7]. *Defense:* When stripped of emotional simulation[cite: 4, 5], separating long-term identity (soul), formatting preferences (heart), reasoning depth (mind), and hard safety rules (doctrine) provides cleaner system prompts than a monolithic instruction block[cite: 6, 7].

---

## 2. Layer Boundaries & Constraints

Canonical flow[cite: 4, 5]:
`Models → Inference Runtime → SkyCore Protocol → Agent Runtime → Memory+Graph+Tools → Orchestrator → CLI`

**Strict Rules:**
*   **No Silent Writes:** File mutation occurs *only* via `apply_diff(signed_token)`[cite: 4, 5].
*   **No Provider Bleed:** CLI and Orchestrator never see OpenAI/Anthropic/llama formats[cite: 4].
*   **No Direct Tool Access:** Agents request tools via SkyCore; Orchestrator executes them[cite: 4].

---

## 3. Core Schemas

### 3.1 SkyCore Protocol (Agent ↔ Inference Boundary)
```json
{
  "task_id": "uuid",
  "agent_id": "coder-primary",
  "goal": "Implement auth middleware",
  "context_refs": ["graph:node:auth.ts", "memory:dec-41"],
  "tools_allowed": ["search_code", "create_diff"],
  "output_contract": "diff_proposal"
}
```

### 3.2 SQLite V1 Schema (Memory + Tools)
```sql
-- Append-only audit log[cite: 4, 5]
CREATE TABLE tool_events (
    id TEXT PRIMARY KEY,
    task_id TEXT,
    tool_name TEXT,
    input_hash TEXT,
    output_hash TEXT,
    status TEXT,
    created_at TIMESTAMP
);

-- FTS5 Memory[cite: 4, 5]
CREATE VIRTUAL TABLE project_memory USING fts5(
    id UNINDEXED, type, content, importance UNINDEXED
);

-- Graph Edges[cite: 4, 6]
CREATE TABLE graph_edges (
    source_id TEXT,
    target_id TEXT,
    relation_type TEXT, -- 'imports', 'calls', 'depends_on'[cite: 6, 8]
    metadata JSON
);
```

---

## 4. Phased Execution Plan

### Phase 1: Safe Tool Spine & Audit Log
*   **Goal:** Build the filesystem interface without AI.
*   **Deliverables:** `read_file`, `tree_sitter` graph parser[cite: 5], `create_diff`, `apply_diff`.
*   **Mechanism:** `create_diff` outputs a visual diff and generates a signed UUID (TTL: 5 mins). `apply_diff` consumes the UUID[cite: 5]. All actions write to `tool_events`[cite: 5].
*   **Boundary Crossing:** `CLI → Orchestrator → Tools → Filesystem`. *Risk:* Unapproved writes. *Control:* `apply_diff` rejects missing/expired tokens.
*   **Hard Exit Gate:** 100 automated test edits run; 0 filesystem writes occur without valid signed approval tokens[cite: 5].

### Phase 2: Memory & Graph Substrate
*   **Goal:** Offline context retrieval[cite: 4].
*   **Deliverables:** SQLite schema, FTS5 keyword indexing[cite: 4, 5]. Structural graph extraction (files, imports, exports, functions)[cite: 4, 8].
*   **Boundary Crossing:** `Orchestrator → Memory/Graph → Filesystem (Read-Only)`. *Risk:* OOM on large repos. *Control:* `tree-sitter` incremental parsing[cite: 5, 6].
*   **Hard Exit Gate:** Graph impact query (`skycode graph impact <symbol>`) correctly identifies downstream dependencies on a real test repository without vector search[cite: 4, 8].

### Phase 3: Inference & SkyCore Protocol
*   **Goal:** Model agnosticism[cite: 6, 7].
*   **Deliverables:** `llama.cpp` local integration[cite: 4, 8]. SkyCore JSON serialization/deserialization[cite: 4].
*   **Boundary Crossing:** `SkyCore Protocol → Inference Runtime → Model`. *Risk:* Provider format bleed. *Control:* Integration tests ensure raw LLM outputs are stripped before passing to the Agent Runtime[cite: 4].
*   **Hard Exit Gate:** A local GGUF model successfully parses a SkyCore request and returns a valid SkyCore response entirely offline[cite: 4, 8].

### Phase 4: The Coder Agent (V1 Integration)
*   **Goal:** End-to-end task loop.
*   **Deliverables:** `coder-primary` instantiation using `soul/heart/mind/doctrine`[cite: 4, 7]. Orchestrator task loop: Classify → Context (Graph/Mem) → Model (SkyCore) → Diff → Approval → Apply → Log[cite: 4].
*   **Boundary Crossing:** `Orchestrator → Agent Runtime → SkyCore`. *Risk:* Agent bypasses orchestrator. *Control:* Agent runtime lacks filesystem permissions; it can only return a `diff_proposal` JSON[cite: 4].
*   **Hard Exit Gate:** User runs `skycode ask "rename X to Y"`. The system reads the graph[cite: 8], proposes a diff[cite: 4], waits for CLI approval[cite: 4], applies it[cite: 4], and recalls the action after a process restart[cite: 4, 8]. No network connection active[cite: 4].