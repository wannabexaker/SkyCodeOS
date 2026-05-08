# Skycode Implementation Plan (Claude POV)

## Verdict on Existing Plans

- **DeepSeek**: visionary but fatally over-scoped. "AI civilization," personality drift, swarm consensus, voice — all premature. Schema and graph thinking are gold; the rest is decoration.
- **ChatGPT**: the strongest engineering spine. Phased, disciplined, correct ordering. Slightly verbose, but the "first useful, then intelligent, then social, then alive" sequencing is the right one.
- **GitHub Copilot**: tightest and most professional. Clear non-negotiables, explicit verification gates, defined exit criteria. Best operational document of the five.
- **Gemini**: a poster, not a plan. Useful as a vision summary, useless as a build doc.
- **Existing /docs**: already aligned with the Copilot/ChatGPT axis. Keep.

The right move is **Copilot's discipline + ChatGPT's phasing + DeepSeek's schemas (stripped) + the existing docs as canonical**.

---

## 1. Core Thesis

Skycode v1 is **one persistent coder agent that edits code safely, remembers across sessions, and retrieves context via a project graph instead of dumping files into prompts.** Everything else is v2+.

If v1 cannot do this reliably on a real codebase, no amount of multi-agent theater will save it.

## 2. Layer Boundaries (frozen)
Models → Inference Runtime → SkyCore Protocol → Agent Runtime
→ Memory + Graph + Tools → Orchestrator → CLI (UI later)

Inviolable rules:
- UI never speaks provider formats. Only SkyCore.
- Agent Runtime never calls providers. Only Model Runtime.
- Every write goes through policy → diff → approval → apply → log.
- Memory is SQLite until measurably broken.

## 3. What to Cut From DeepSeek

Cut entirely from v1 scope: emotional valence, personality drift, trust scores between agents, swarm execution, consensus voting, voice, vector DB, multimodal, evolution.log, the five-agent colony, "agent sovereignty" rhetoric.

Keep the schemas (episodic_memory, knowledge_edges, relationship_memory) as **future migration targets**, not v1 tables.

## 4. Revised Phase Plan (12 weeks, hard gates)

**Phase 0 — Freeze (week 0-1)**
Lock /docs as canonical. Resolve any contradictions between memory-system.md and protocol.md. One source of truth, zero parallel plans.

**Phase 1 — Tool Spine (week 1-3)**
Read tools, search, git_status, diff generator, approval gate, apply, rollback, immutable tool event log. No LLM yet. Test on real repos.
*Gate: zero unapproved writes across 50 simulated edits.*

**Phase 2 — Local Inference (week 3-5)**
llama.cpp integration, GGUF loader, streaming, model registry, one OpenAI-compatible remote adapter. No router yet — manual model selection.
*Gate: same prompt produces output across local + remote with identical SkyCore response shape.*

**Phase 3 — Single Coder Agent (week 5-8)**
soul/heart/mind/doctrine (minimal fields only, per agent-definition.md). Task loop. SQLite memory: project, agent, decision, session. Keyword + recency + importance retrieval.
*Gate: agent recalls a decision made in session 1 during session 3, after a process restart.*

**Phase 4 — Graph v1 (week 8-10)**
File/symbol/import/export index. tree-sitter for top 3 languages user actually uses. Impact-analysis query: "what breaks if I change X."
*Gate: graph retrieval reduces context tokens by ≥50% on a real edit task vs naive file dump.*

**Phase 5 — Router + Reviewer (week 10-12)**
Task classifier → model selection → fallback chain. Add reviewer agent. Coder proposes, reviewer critiques, human approves.
*Gate: end-to-end task completes with reviewer catching at least one real issue across a test suite of 20 tasks.*

**UI is Phase 6+. Not in v1.**

## 5. Key Technical Decisions (opinionated)

- **Rust for runtime, Python for agent definitions** is fine, but pick one and don't drift. All-Rust is cleaner; Python tempts you toward LangChain-flavored mush.
- **SQLite FTS5 first.** Don't touch embeddings until FTS5 demonstrably fails on retrieval quality benchmarks you've actually run.
- **tree-sitter, not custom parsing.** Don't reinvent.
- **Approval token = signed UUID with TTL**, not a boolean. Prevents replay and stale approvals.
- **Tool event log is append-only and content-addressable.** This is your audit spine. Do not skimp.
- **Model registry as YAML, hot-reloadable.** No hardcoded model names anywhere in code.

## 6. What Success Looks Like at v1

A user runs `skycode` on a 50k-line codebase. The agent reads the graph, proposes a refactor as a diff, the user approves, the change applies, and three sessions later the agent references *why* it made that choice. No remote API required. No silent writes ever occurred.

That's it. That's v1. Everything DeepSeek wanted comes after.