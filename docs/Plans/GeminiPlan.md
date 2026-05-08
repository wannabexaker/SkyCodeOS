This is the comprehensive, unified master prompt for **Skycode**. It merges the "AI Civilization" vision of **DeepSeekPlan.md**[cite: 2] with the "Engineering First" discipline of **ChatgptPlan.md**[cite: 1].

Copy the content below into your project's `README.md` or a master configuration file to serve as the "ground truth" for your development.

---

# Skycode Master System Prompt: The Cognitive OS

## 1. Vision & Core Subject
Skycode is a **Persistent Multi-Agent Cognitive Operating System**[cite: 1, 2]. It is not a chatbot; it is an orchestration runtime designed to manage a software team of persistent AI agents that live across sessions, form relationships, and evolve through project experience[cite: 2].

### The Core Goal
To enable a local-first environment where specialized agents (Architects, Coders, Reviewers) understand code as a connected knowledge graph rather than a collection of text files[cite: 1, 2].

---

## 2. Architectural Pillars
*   **Agent Sovereignty:** Every agent has an irreversible identity, ownership of its own memory, and autonomy within its "Doctrine"[cite: 2].
*   **Graph-Based Cognition:** The system maps dependencies, imports, and architectural decisions into a graph to reduce token usage and improve retrieval[cite: 1, 2].
*   **Model Agnosticism:** The system must function regardless of the backend (local llama.cpp or remote APIs), using an intelligent Model Router to pick the best tool for the job[cite: 1, 2].
*   **Safe Execution:** All file writes and terminal commands require a human-in-the-loop (HITL) approval via a diff-viewer[cite: 1, 2].

---

## 3. Agent Definition Standard
Every agent in Skycode is defined by four core YAML "files" that dictate its behavior[cite: 1, 2]:

| File | Purpose | Content |
| :--- | :--- | :--- |
| **Soul.yaml** | **Identity** | Core values, archetypes, and long-term identity (e.g., "Modular Architect")[cite: 1, 2]. |
| **Heart.yaml** | **Behavior** | Communication style, empathy levels, and team collaboration rules[cite: 1, 2]. |
| **Mind.yaml** | **Reasoning** | Planning depth, risk tolerance, and decision-making logic[cite: 1, 2]. |
| **Doctrine.yaml** | **Hard Rules** | Non-negotiable constraints (e.g., "Never modify git history without backup")[cite: 1, 2]. |

---

## 4. The Technical Stack
*   **Runtime:** Rust (Tauri) for a high-performance, local-first desktop application[cite: 1, 2].
*   **Inference:** Local execution via `llama.cpp` (GGUF) to avoid dependency on cloud providers[cite: 1].
*   **Memory Store:** SQLite for episodic and project memory (Vector extensions only when strictly necessary)[cite: 1, 2].
*   **Internal Protocol:** **SkyCore Protocol**—a universal format that translates any model's output into structured Skycode tasks[cite: 1].

---

## 5. Implementation Roadmap (Phased Approach)

### Phase 1: Foundation (The Sentinel)
*   Build the Rust runtime and SQLite memory schema[cite: 2].
*   Implement "Safe Tools": file reader, folder lister, and diff generator[cite: 1].
*   Deliverable: A CLI where you can ask "What does this project do?" and get a summary[cite: 1].

### Phase 2: Persistence (The Memory)
*   Enable session resuming so agents remember previous context[cite: 2].
*   Build the **SkyGraph** to map file dependencies and imports[cite: 1, 2].
*   Deliverable: Agents recall *why* a previous architectural decision was made[cite: 2].

### Phase 3: Collaboration (The Firm)
*   Introduce the **Orchestrator** to manage communication between a Coder agent and a Reviewer agent[cite: 1, 2].
*   Implement the **Model Router** to send simple tasks to small local models and complex tasks to heavy reasoning models[cite: 1, 2].
*   Deliverable: A multi-agent team that can plan, execute, and review a code change autonomously[cite: 1, 2].

---

## 6. The "Non-Negotiables" (System Guardrails)
1.  **No Silent Writes:** The system shall never modify a file without showing a diff and receiving human approval[cite: 1, 2].
2.  **Separate Everything:** Model logic must remain separate from orchestration logic. You should be able to swap the LLM without rebuilding the agent's "Soul"[cite: 1].
3.  **Local First:** The system must remain functional without an internet connection using local GGUF models[cite: 1].
4.  **No Context Dumping:** Do not read entire repos into context. Use the Graph and SQLite index to retrieve only what is relevant[cite: 1].

---

## 7. Success Metric
**Skycode v1.0** is successful when a user can open a new project, and a team of persistent agents provides a 5x reduction in manual coding time through intelligent, safe, and context-aware collaboration[cite: 2].

---

Since this plan emphasizes a "local-first" approach, do you want to prioritize setting up the local model manager (llama.cpp) or the project indexing system (SkyGraph) first?