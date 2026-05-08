# Skycode Professional R&D Plan

## Project Name

Skycode

## Project Type

Persistent Multi-Agent Cognitive Operating System

## Core Subject

Skycode is a modular AI runtime for building persistent coder agents, multi-agent collaboration, project memory, model routing, local/remote model execution, tool usage, and graph-based project understanding.

Skycode is not just a chatbot.
It is an orchestration system above AI models.

---

# 1. Core Vision

Skycode should become an AI team operating system.

The user should be able to open a project and interact with multiple AI agents that behave like a real software team:

Architect, backend developer, frontend developer, reviewer, security analyst, DevOps engineer, researcher, manager.

Each agent should have identity, role, memory, behavior, skills, tools, project knowledge, and collaboration rules.

The system should remember across sessions, understand projects structurally, reduce token usage, route tasks to the correct model, and safely read/write code.

---

# 2. Main Principle

Separate everything.

Model is not agent.
Agent is not runtime.
Runtime is not UI.
Memory is not prompt.
Tools are not skills.
Orchestration is not inference.

Correct structure:

```text
Models
  ↓
Inference Runtime
  ↓
SkyCore Protocol
  ↓
Agent Runtime
  ↓
Memory + Graph + Tools
  ↓
Orchestrator
  ↓
UI / CLI / Voice
```

---

# 3. Main Architecture

## 3.1 SkyModel Runtime

Purpose:

Run local and remote models without depending permanently on Ollama.

Early stage:

Use llama.cpp with GGUF models.

Later:

Support vLLM, OpenAI-compatible APIs, OpenRouter, Gemini, Claude, Grok, local APIs.

Responsibilities:

```text
download models
load models
configure models
stream tokens
manage GPU/CPU settings
manage context size
apply prompt templates
expose internal API
cache loaded models
fallback between models
```

Important:

Do not build your own inference engine now.
Use llama.cpp first. Build your own manager around it.

---

## 3.2 SkyCore Protocol

Purpose:

Universal internal format for all models, agents, tools, memory, and tasks.

Every provider must adapt into SkyCore.

Example:

```json
{
  "task_id": "uuid",
  "agent_id": "backend-dev",
  "role": "Backend Developer",
  "goal": "Refactor auth service",
  "context_refs": ["memory:123", "file:src/auth.ts"],
  "tools_allowed": ["read_file", "write_diff", "terminal"],
  "model_policy": {
    "preferred": "local-coder",
    "fallback": "remote-strong"
  },
  "output_contract": "diff_with_explanation"
}
```

This prevents your frontend from depending on Claude/Ollama/OpenAI formats.

---

## 3.3 SkyAgent Runtime

Purpose:

Run agents as persistent entities.

Each agent has:

```text
soul
heart
mind
doctrine
skills
tools
memory
relationships
state
metrics
```

Recommended agent folder:

```text
/agents
  /architect
    /core
      soul.yaml
      heart.yaml
      mind.yaml
      doctrine.yaml
    /capabilities
      skills.yaml
      tools.yaml
      permissions.yaml
    /memory
      memory.sqlite
      relationships.sqlite
    /state
      current.yaml
      metrics.sqlite
      evolution.log
```

---

# 4. Agent Instruction Categories

## soul.yaml

Purpose:

Identity.

Defines who the agent is.

Contains:

```text
name
role
archetype
core values
likes
dislikes
strengths
weaknesses
long-term identity
```

This should change rarely.

---

## heart.yaml

Purpose:

Behavior.

Defines how the agent speaks, collaborates, reacts, handles conflict, and behaves under pressure.

Contains:

```text
communication style
team behavior
conflict behavior
patience
assertiveness
empathy
criticism style
collaboration attitude
```

---

## mind.yaml

Purpose:

Thinking style.

Defines how the agent reasons.

Contains:

```text
planning depth
risk tolerance
validation style
debugging style
creativity level
decision process
review behavior
```

---

## doctrine.yaml

Purpose:

Hard rules.

Defines what the agent must never do and what it must always do.

Contains:

```text
safety rules
file modification rules
git rules
approval requirements
forbidden actions
priority order
```

Example:

```yaml
must_never:
  - delete files without approval
  - force push git history
  - run unknown scripts without confirmation

must_always:
  - create diffs before writing
  - preserve existing architecture
  - log major decisions
```

---

## skills.yaml

Purpose:

Reusable abilities.

Skills are behavioral patterns, not executable tools.

Examples:

```text
architecture_review
debugging
refactoring
security_review
readme_writer
planning
test_generation
code_explanation
```

---

## tools.yaml

Purpose:

Executable capabilities.

Tools actually do things.

Examples:

```text
read_file
write_file
create_diff
run_terminal
git_status
git_commit
search_project
open_url
run_tests
```

---

## relationships.yaml / relationships.sqlite

Purpose:

Agent-to-agent collaboration memory.

Contains:

```text
trust
past cooperation
conflicts
preferred collaborators
review history
who is strong at what
```

This is good, but should stay controlled.
Do not make it too “emotional” early. First make it useful.

---

# 5. Memory System

Start simple. Do not overbuild.

## Required memory scopes

```text
global memory
project memory
agent memory
session memory
relationship memory
decision memory
```

## Early implementation

Use SQLite first.

Tables:

```text
memories
projects
agents
decisions
files_index
tool_events
agent_events
relationships
```

Do not start with vector DB immediately.

First version:

```text
keyword search
recency
importance score
manual tags
project_id
agent_id
```

Later:

```text
embeddings
semantic search
vector similarity
GraphRAG
memory compression
```

---

# 6. Graph System

This is critical.

The graph prevents agents from reading the entire project every time.

## SkyGraph should understand:

```text
files
folders
imports
exports
classes
functions
dependencies
routes
services
components
database models
config files
tests
architecture decisions
```

Example:

```text
AuthController
  calls → AuthService
  uses → UserRepository
  depends_on → JwtModule
  tested_by → auth.spec.ts
```

## First graph version

Use simple project indexing:

```text
file path
language
imports
exports
symbols
last modified
summary
```

## Later graph version

Use tree-sitter.

Then build AST-level project intelligence.

---

# 7. Tool System

This is the real first MVP.

Before agents, memory, UI, voice, or model routing, Skycode must do this reliably:

```text
read files
list folders
search project
create diff
apply diff
run terminal command
show git status
rollback changes
```

Rules:

```text
no silent writes
human approval for file edits
diff before apply
logs for every tool action
safe command allowlist
git branch before risky edits
```

---

# 8. Orchestration System

Purpose:

Decide who does what.

The orchestrator should:

```text
understand task type
select agent
select model
load memory
load graph context
choose tools
execute workflow
request review
store decision
return result
```

Basic workflow:

```text
User request
  ↓
Task classifier
  ↓
Context builder
  ↓
Agent selector
  ↓
Model router
  ↓
Tool execution
  ↓
Review
  ↓
Memory update
```

Do not start with swarm behavior.
Start with one manager and one coder.

---

# 9. Model Router

Purpose:

Choose the best model for each job.

Router should know:

```text
model name
provider
cost
speed
context window
coding ability
reasoning ability
vision support
tool calling support
local/remote
availability
```

Example routing:

```text
simple summary → cheap local model
code edit → coder model
architecture planning → strong reasoning model
large context → Gemini/long-context model
fast file classification → small local model
```

Do not hardcode models. Use registry.

Example:

```yaml
models:
  qwen-coder-7b:
    provider: local
    runtime: llamacpp
    strengths: [coding, refactor]
    context: 32768
    cost: free
    speed: medium

  gemini-free:
    provider: remote
    strengths: [long_context, summarization]
    cost: free_limited
```

---

# 10. UI Plan

UI comes after runtime works.

Required UI panels:

```text
chat
agents
tasks
memory
project graph
tool logs
model manager
settings
diff approval
```

Recommended stack:

```text
Tauri
React
TypeScript
Rust backend
SQLite
```

Do not build beautiful UI first.
Build working runtime first.

---

# 11. MVP Roadmap

## Phase 0 — Research and Architecture

Deliverables:

```text
docs/architecture.md
docs/protocol.md
docs/agent-definition.md
docs/memory-system.md
docs/tool-system.md
docs/model-runtime.md
docs/roadmap.md
```

Goal:

Understand and freeze the design before coding more chaos.

---

## Phase 1 — Project Reader

Build:

```text
CLI command
project scanner
file reader
folder lister
basic project summary
SQLite project index
```

Example:

```bash
skycode scan ./my-project
skycode ask "what is this project?"
```

Success:

Skycode can read and summarize a project.

---

## Phase 2 — Safe Writer

Build:

```text
diff generator
approval system
apply patch
rollback
git status
change log
```

Example:

```bash
skycode ask "rename this function safely"
```

Success:

Skycode proposes a diff, waits for approval, applies it safely.

---

## Phase 3 — Single Coder Agent

Build:

```text
one agent
soul/heart/mind/doctrine files
basic memory
tool permissions
task execution loop
```

Agent:

```text
coder-primary
```

Success:

One persistent coder agent can read, reason, propose, edit, and remember.

---

## Phase 4 — Memory

Build:

```text
project memory
agent memory
decision memory
session resume
important fact extraction
```

Success:

Agent remembers previous decisions after restart.

---

## Phase 5 — Basic Graph

Build:

```text
file graph
import graph
symbol index
dependency map
```

Success:

Agent can answer:

```text
what files are affected if I change this module?
```

---

## Phase 6 — Multi-Agent

Add:

```text
architect
coder
reviewer
manager
```

Workflow:

```text
manager plans
architect validates
coder edits
reviewer checks
human approves
```

Success:

Multiple agents collaborate on one code task.

---

## Phase 7 — Model Runtime Without Ollama

Build:

```text
llama.cpp integration
GGUF loader
model registry
streaming output
model config
hardware detection
```

Success:

Skycode runs local GGUF models without Ollama.

---

## Phase 8 — Model Router

Build:

```text
model registry
task classifier
fallback chain
local/remote routing
free-model usage strategy
```

Success:

Skycode chooses the correct model automatically.

---

## Phase 9 — UI

Build:

```text
Tauri app
agent chat
diff viewer
memory viewer
graph viewer
model manager
settings
```

Success:

Usable desktop AI team interface.

---

## Phase 10 — Voice and Multimodal

Add later:

```text
TTS
STT
image input
screenshots
vision models
voice commands
```

Not early.

---

# 12. Non-Negotiable Rules

```text
Do not build UI before core runtime.
Do not build multi-agent before single-agent works.
Do not build vector DB before SQLite is insufficient.
Do not allow silent file writes.
Do not mix model runtime with orchestration.
Do not depend permanently on Ollama.
Do not hardcode provider-specific formats into the UI.
Do not store giant prompts as one file.
Do not read whole repos when graph/context retrieval can work.
Do not allow agents to bypass the orchestrator.
```

---

# 13. What To Keep From DeepSeek Plan

Keep:

```text
persistent agents
agent identity folders
soul/heart/mind/doctrine split
relationship memory
graph cognition
model agnosticism
Rust runtime
SQLite first
safe tools
approval system
multi-agent collaboration
model router
Tauri UI later
```

Modify:

```text
“AI civilization” should be vision language, not MVP architecture.
Do not start with emotional/personality drift.
Do not start with swarm execution.
Do not start with full autonomy.
Do not start with voice.
Do not start with vector DB.
Do not start with five agents.
```

The DeepSeek plan is strong, but too big too early.

Correct strategy:

```text
first make it useful
then make it intelligent
then make it social
then make it alive
```

---

# 14. Final Target

Skycode v1 should be:

```text
A local-first AI coder operating system where persistent agents can read, understand, edit, review, remember, and collaborate across software projects using local and remote models through a clean orchestration layer.
```

The first real product is not the full civilization.

The first real product is:

```text
Persistent Coder Agent + Safe File Editing + Project Memory + Graph Context
```

Everything else grows from that.