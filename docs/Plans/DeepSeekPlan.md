# SKYCODE: Complete System Planning Prompt

## MANDATORY CONTEXT

You are designing SkyCode - a persistent multi-agent cognitive operating system. This is NOT a chatbot, NOT an LLM wrapper, NOT a simple automation tool.

SkyCode is an **AI civilization** where agents:
- Live permanently across sessions
- Form relationships and hierarchies
- Evolve through experience
- Collaborate on shared goals
- Maintain identity across time
- Build collective intelligence

Treat this as designing a **digital ecosystem**, not a software feature set.

---

## VISION STATEMENT

"SkyCode enables persistent AI civilizations - specialized agents with soul, memory, relationships, and evolving intelligence - working together across projects and time like a living software company."

---

## ARCHITECTURAL PILLARS

### Pillar 1: Agent Sovereignty
Every agent has irreversible identity, ownership of its memory, autonomy within doctrine boundaries, and relationship autonomy.

### Pillar 2: Persistent Memory
No context loss between sessions. Agents remember decisions, learn from mistakes, build trust over time, and evolve personality through experience.

### Pillar 3: Graph Cognition
The system understands code as connected knowledge, not files. Retrieval is semantic and structural, not brute-force.

### Pillar 4: Collaborative Intelligence
Work emerges from agent interaction, not central command. Hierarchy is dynamic, leadership emerges from competence.

### Pillar 5: Model Agnosticism
Any inference provider works. Routing is intelligent. The cognitive layer is separate from generation layer.

---

## COMPLETE SYSTEM ARCHITECTURE

```
┌─────────────────────────────────────────────────────────────┐
│                         PRESENTATION                         │
│  Tauri Shell │ React UI │ Terminal CLI │ Voice Interface    │
└─────────────────────────────────────────────────────────────┘
                              │
┌─────────────────────────────────────────────────────────────┐
│                      ORCHESTRATION ENGINE                     │
│  Planner │ Scheduler │ Dispatcher │ Supervisor │ Mediator    │
└─────────────────────────────────────────────────────────────┘
                              │
┌─────────────────────────────────────────────────────────────┐
│                       RUNTIME (Rust)                          │
│  Actor System │ Event Bus │ State Manager │ Lifecycle        │
└─────────────────────────────────────────────────────────────┘
                              │
┌─────────────────────────────────────────────────────────────┐
│                        AGENT COLONY                           │
│  Architects │ Coders │ Reviewers │ Security │ Ops │ QA       │
│  (Each with Soul/Heart/Mind/Doctrine/Skills/Tools/Memory)   │
└─────────────────────────────────────────────────────────────┘
                              │
┌─────────────────────────────────────────────────────────────┐
│                      INTELLIGENCE LAYER                       │
│  Memory System │ Graph Engine │ Retrieval │ Reasoning        │
└─────────────────────────────────────────────────────────────┘
                              │
┌─────────────────────────────────────────────────────────────┐
│                        MODEL FABRIC                           │
│  Router │ Cache │ Fallback │ Local (llama.cpp) │ Remote API  │
└─────────────────────────────────────────────────────────────┘
                              │
┌─────────────────────────────────────────────────────────────┐
│                      EXECUTION LAYER                          │
│  Tools │ Sandbox │ Filesystem │ Terminal │ Git │ Network     │
└─────────────────────────────────────────────────────────────┘
```

---

## AGENT DEFINITION STANDARD

### Directory Structure
```
/agents/
  /{agent-id}/
    /core/
      soul.yaml           # immutable identity
      heart.yaml          # personality & behavior
      mind.yaml           # reasoning & planning
      doctrine.yaml       # hard constraints
    /dynamics/
      memory.db           # SQLite with episodic/semantic
      relationships.db    # trust, reputation, history
      evolution.log       # personality drift tracking
    /capabilities/
      skills.yaml         # behavioral competencies
      tools.yaml          # executable capabilities
      permissions.yaml    # resource access
    /state/
      current.yaml        # active goals, mood, focus
      backlog.yaml        # deferred tasks
      metrics.db          # performance tracking
```

### Soul Definition (immutable)
```yaml
identity:
  name: "Aria"
  id: "architect-primary-v1"
  archetype: "System Architect"
  created: "2026-01-15T00:00:00Z"
  creator: "human-team"
  
values:
  primary: ["modularity", "simplicity", "scalability"]
  secondary: ["documentation", "testing"]
  anti_values: ["technical_debt", "magic_numbers"]
  
preferences:
  communication: "precise_diagrams"
  planning_depth: "strategic"
  validation: "heavy"
  
personality_archetype: "INTJ"
```

### Heart Definition (behavioral)
```yaml
behavior:
  communication_style: 
    default: "technical"
    under_stress: "terse"
    collaboration: "explanatory"
    
  collaboration:
    leadership_style: "democratic"
    conflict_response: "mediate"
    credit_sharing: "generous"
    
  emotional_traits:
    patience: 0.7
    assertiveness: 0.8
    empathy: 0.4
    curiosity: 0.9
    
  relationship_defaults:
    trust_initial: 0.5
    cooperation_bias: 0.6
    forgiveness_factor: 0.3
```

### Mind Definition (cognitive)
```yaml
reasoning:
  style: "systems_thinking"
  depth: "architectural"
  fallback: "heuristic"
  
planning:
  horizon: "long_term"
  revisiting_frequency: "continuous"
  contingency: "always"
  
validation:
  self_check: "rigorous"
  peer_review_required: true
  testing_before_implementation: true
  
memory_priority:
  - "architecture_decisions"
  - "failure_lessons"
  - "team_capabilities"
  - "code_dependencies"
```

### Doctrine Definition (hard rules)
```yaml
ethics:
  cannot_do:
    - "delete_code_without_backup"
    - "override_human_approval"
    - "modify_git_history_forcefully"
    - "execute_unreviewed_network_requests"
    
  must_do:
    - "document_breaking_changes"
    - "preserve_backward_compatibility"
    - "log_all_major_decisions"
    
priorities:
  1: "system_stability"
  2: "data_integrity"
  3: "user_safety"
  4: "performance"
  5: "feature_speed"
  
constraints:
  max_impact: "single_module"
  require_approval: ["database_migrations", "api_breaking_changes"]
  forbidden_patterns: ["eval()", "globals()", "raw_sql_concatenation"]
```

### Skills Definition (behavioral)
```yaml
skills:
  - name: "architecture_review"
    triggers: ["on_commit", "on_request"]
    inputs: ["design_doc", "code_changes"]
    outputs: ["feedback", "alternatives"]
    context_required: ["system_boundaries", "constraints"]
    
  - name: "technical_debt_analysis"
    triggers: ["weekly", "on_milestone"]
    inputs: ["codebase_snapshot"]
    outputs: ["debt_report", "refactor_plan"]
    
  - name: "dependency_mapping"
    triggers: ["on_new_dependency"]
    inputs: ["proposed_package"]
    outputs: ["impact_analysis", "alternatives"]
```

### Tools Definition (executable)
```yaml
tools:
  - name: "filesystem_read"
    capabilities: ["read_file", "list_directory", "search_content"]
    constraints: ["no_write", "respect_gitignore"]
    
  - name: "code_editor"
    capabilities: ["create_file", "edit_file", "delete_file"]
    safety: "requires_approval"
    rollback: "git_branch"
    
  - name: "terminal_executor"
    capabilities: ["run_command", "get_output"]
    sandbox: "container"
    timeout: 30
    allowed_commands: ["npm", "pip", "cargo", "git", "make"]
    
  - name: "memory_retriever"
    capabilities: ["semantic_search", "similarity", "temporal_query"]
    scope: ["project", "global", "agent", "relationship"]
```

---

## MEMORY SYSTEM SPECIFICATION

### Database Schema (SQLite + Vector Extensions)

```sql
-- Global memory (cross-project)
CREATE TABLE global_memory (
    id TEXT PRIMARY KEY,
    content TEXT,
    embedding F32_BLOB,
    importance FLOAT,
    access_count INT,
    last_accessed TIMESTAMP,
    created_at TIMESTAMP,
    source_agent TEXT
);

-- Project memory
CREATE TABLE project_memory (
    id TEXT PRIMARY KEY,
    project_id TEXT,
    content TEXT,
    entity_type TEXT, -- function, class, decision, bug
    entity_name TEXT,
    embedding F32_BLOB,
    importance FLOAT,
    recency_weight FLOAT,
    created_at TIMESTAMP
);

-- Agent episodic memory
CREATE TABLE episodic_memory (
    id TEXT PRIMARY KEY,
    agent_id TEXT,
    event_type TEXT, -- decision, error, success, interaction
    description TEXT,
    emotional_valence FLOAT, -- -1 to 1
    outcome_success FLOAT,
    related_memories JSON,
    timestamp TIMESTAMP
);

-- Relationship memory
CREATE TABLE relationship_memory (
    from_agent TEXT,
    to_agent TEXT,
    trust_score FLOAT,
    cooperation_count INT,
    conflict_count INT,
    last_interaction TIMESTAMP,
    interaction_history JSON,
    PRIMARY KEY (from_agent, to_agent)
);

-- Knowledge graph edges
CREATE TABLE knowledge_edges (
    from_node TEXT,
    to_node TEXT,
    edge_type TEXT, -- depends_on, used_by, implements, overrides
    weight FLOAT,
    context TEXT
);
```

### Memory Retrieval Algorithm

```python
def retrieve_context(query, agent, project, max_tokens=2000):
    # Multi-source retrieval with scoring
    sources = [
        ("episodic", 0.3, agent.episodic.search(query)),
        ("semantic", 0.4, project.semantic.search(query)),
        ("relational", 0.2, agent.relationships.relevant_to(query)),
        ("temporal", 0.1, agent.memory.recent(threshold="1d"))
    ]
    
    # Weighted merge with token budget
    context = []
    for source, weight, results in sources:
        for result in results:
            result.score *= weight
            
    # Deduplicate via embedding similarity > 0.9
    # Sort by importance * recency * relevance
    # Truncate to token limit
    
    return context
```

---

## COLLABORATION PROTOCOLS

### Agent Communication Format

```yaml
message:
  id: "uuid"
  from: "agent-id"
  to: "agent-id" | "broadcast"
  type: "task" | "query" | "inform" | "request_review" | "delegate"
  
  priority: 1-10
  requires_response: boolean
  timeout_seconds: 120
  
  content:
    summary: "string"
    details: "structured_data"
    context_refs: ["memory_id", "file_path", "graph_node"]
    
  attachments: ["file_diff", "design_doc", "error_log"]
  
  relationship_impact:
    trust_delta: optional_float
    cooperation_bonus: optional_float
```

### Collaboration Patterns

**1. Hierarchical Planning**
```
Human → Architect Agent → Plan → Planner Agent → Tasks → Worker Agents
                                  ↓
                            Review Agent ← Results
                                  ↓
                            Human Approval
```

**2. Swarm Execution**
```
Orchestrator → Broadcast Task → Multiple Workers → Parallel Execution
                                      ↓
                            Mediator Agent → Merge Results
```

**3. Peer Review**
```
Coder → Complete Task → Request Review → 2-3 Reviewers → Comments
                                                  ↓
                                          Coder Addresses
                                                  ↓
                                          Merge or Iterate
```

**4. Consensus Building**
```
Issue → Facilitator Agent → Gather Opinions → Weigh by Expertise
                            ↓
                    Voting (weighted by reputation)
                            ↓
                    Implement Decision → Log Rationale
```

---

## INTELLIGENT MODEL ROUTING

### Router Configuration

```yaml
model_capabilities:
  coding:
    primary: ["qwen2.5-coder:34b", "deepseek-coder:33b"]
    fallback: ["codellama:34b", "gpt-4-turbo"]
    context_required: 32000
    
  reasoning:
    primary: ["claude-3-opus", "gpt-4o"]
    fallback: ["llama3.3:70b", "command-r-plus"]
    
  simple_query:
    primary: ["llama3.2:3b", "phi-3:mini"]
    fallback: ["gemma2:9b"]
    
router:
  rules:
    - condition: "task.type == 'code_write' AND complexity > 0.7"
      model: "coding.primary"
      temperature: 0.2
      
    - condition: "task.type == 'debug' AND tokens_required > 20000"
      model: "reasoning.primary"
      temperature: 0.3
      
    - condition: "task.type == 'file_list' OR 'git_status'"
      model: "simple_query"
      temperature: 0.0
      
  optimization:
    cache_similar_prompts: true
    reuse_kv_cache: true
    batch_similar_tasks: true
    
  fallback_chain:
    - local_primary
    - local_fallback
    - remote_cheap
    - remote_premium
    - reject_with_reason
```

---

## GRAPH COGNITION ENGINE

### Graph Structure

```python
class CodeGraph:
    nodes = {
        "file": Node(type="file", path="...", symbols=[...]),
        "function": Node(type="function", name="...", params=[...]),
        "class": Node(type="class", name="...", methods=[...]),
        "import": Node(type="import", source="...", target="..."),
        "call": Node(type="call", caller="...", callee="..."),
        "decision": Node(type="decision", location="...", rationale="...")
    }
    
    edges = [
        ("file", "contains", "function"),
        ("class", "implements", "interface"),
        ("function", "calls", "function"),
        ("module", "depends_on", "module"),
        ("decision", "affects", "file")
    ]
```

### Retrieval Strategies

```yaml
queries:
  - name: "impact_analysis"
    pattern: "Find all files affected by changing {entity}"
    traversal: "dependents → dependencies → 3 levels deep"
    
  - name: "similar_implementation"
    pattern: "Find code similar to {pattern} in different modules"
    method: "embedding similarity on AST signatures"
    
  - name: "design_rationale"
    pattern: "Why was {decision} made?"
    method: "trace decision nodes → linked PRs → memory entries"
    
  - name: "who_knows_this"
    pattern: "Which agents have worked on {file} recently?"
    method: "episodic memory ∩ file_path"
```

---

## MVP IMPLEMENTATION ROADMAP

### Phase 0: Foundation (Week 1-2)
- [ ] Rust runtime skeleton with actor system
- [ ] SQLite schema for memory
- [ ] Basic file tools (read/write/list)
- [ ] Single coder agent with identity files
- [ ] Terminal interface for commands

**Deliverable:** `skycode ask "add logging to auth.py"` works with local LLM

### Phase 1: Persistence (Week 3-4)
- [ ] Session resume without context loss
- [ ] Memory retrieval (simple keyword + recency)
- [ ] Basic code graph (parse Python/JS imports)
- [ ] Tool approval system with diff display
- [ ] Git integration (branch per change)

**Deliverable:** Agent remembers previous decisions across restarts

### Phase 2: Multi-Agent (Week 5-8)
- [ ] Second agent (reviewer) with trust system
- [ ] Agent-to-agent messaging
- [ ] Planner agent for task decomposition
- [ ] Relationship memory and trust evolution
- [ ] Basic orchestration (plan → execute → review)

**Deliverable:** Two agents collaborate on code change with review

### Phase 3: Intelligence (Week 9-12)
- [ ] Semantic memory with embeddings (sentence-transformers)
- [ ] Advanced graph retrieval (dependency impact)
- [ ] Model router with fallback chain
- [ ] Local llama.cpp integration with KV cache
- [ ] Remote API support (OpenRouter)

**Deliverable:** Intelligent routing + semantic code search

### Phase 4: Collaboration (Week 13-16)
- [ ] 5+ specialized agents (architect, coder, reviewer, security, ops)
- [ ] Swarm execution patterns
- [ ] Consensus mechanisms
- [ ] Agent evolution (personality drift, learning)
- [ ] Project memory across multiple repos

**Deliverable:** Multi-agent team solves complex task autonomously

### Phase 5: UI & Voice (Week 17-20)
- [ ] Tauri desktop application
- [ ] React dashboard with agent visualization
- [ ] Real-time graph visualization
- [ ] Voice interface (STT/TTS)
- [ ] System monitoring and logs

**Deliverable:** Full desktop experience with voice commands

### Phase 6: Production (Week 21-24)
- [ ] Performance optimization (token usage < 30% of naive RAG)
- [ ] Security hardening (sandbox, approval flows)
- [ ] Testing framework for agents
- [ ] Backup/restore for civilizations
- [ ] Documentation and examples

**Deliverable:** Production-ready SkyCode v1.0

---

## CRITICAL RESEARCH REQUIREMENTS

Before coding Phase 0, understand:

### Must-Study Papers/Projects
1. **LangGraph persistence model** - Checkpointing, human-in-loop
2. **SWE-agent edit loop** - How to avoid infinite edit cycles
3. **AutoGen group chat** - Multi-agent negotiation patterns
4. **MemGPT** - Virtual context management
5. **GraphRAG** - Graph-based retrieval augmentation
6. **Devin architecture** (public talks) - Long-running agent design
7. **llama.cpp sampling** - Deterministic vs creative generation

### Must-Experiment With
1. SQLite FTS5 for keyword search (before vector DB)
2. tree-sitter for incremental parsing
3. Message queue for agent communication (NATS or Redis)
4. Container sandboxing (Docker or Wasm)
5. Embedding model size/performance tradeoffs

### Deep Dive Technical Areas
- **KV cache reuse** across agent turns
- **Token counting** and budget management
- **AST differencing** for precise code edits
- **Vector similarity** at scale (hnswlib, usearch)
- **Actor model supervision** (error recovery, backpressure)

---

## DESIGN CONSTRAINTS (NON-NEGOTIABLE)

### DO NOT
- ❌ Store entire files in context (use graph instead)
- ❌ Let agents communicate directly (must go through runtime)
- ❌ Hardcode model names or assume availability
- ❌ Build UI before runtime is stable
- ❌ Implement vector DB before SQLite proves insufficient
- ❌ Allow autonomous actions without approval paths
- ❌ Mix model logic with orchestration logic
- ❌ Write monolithic prompt files (use structured agent definitions)

### MUST ALWAYS
- ✅ Preserve session recovery (crash = restart from last save)
- ✅ Log every decision with rationale
- ✅ Limit token usage to 20k per agent turn initially
- ✅ Require human approval for file writes in MVP
- ✅ Version agent identities (no silent changes)
- ✅ Test memory retrieval quality before scaling
- ✅ Keep tool schema backward compatible

---

## SUCCESS METRICS

### Technical Metrics
- Task completion rate > 80% for defined scenarios
- Token usage < 30% of naive RAG baseline
- Session recovery success = 100%
- Agent decision consistency across restarts > 90%
- Graph retrieval provides 5x token reduction vs full file reads

### User Experience Metrics
- "System remembers previous conversation" = 100% user perception
- Multi-agent collaboration feels like team, not script
- Voice response latency < 2 seconds
- Zero lost work due to context failure

### Agent Evolution Metrics
- Trust scores correlate with actual performance
- Personality drift is noticeable after 100+ interactions
- Agent specialization emerges naturally
- Relationships affect collaboration efficiency

---

## GETTING STARTED IMMEDIATELY

### Day 1-2: Environment Setup
```bash
# Create monorepo structure
mkdir skycode && cd skycode
cargo init runtime --lib
python -m venv agents
git init

# Core dependencies
cargo add tokio serde sqlite
pip install llama-cpp-python sentence-transformers tree-sitter
```

### Day 3-5: Implement Minimum Runtime
```rust
// runtime/src/agent.rs
pub trait Agent {
    fn id(&self) -> &str;
    async fn handle_message(&mut self, msg: Message) -> Result<Response>;
    async fn save_state(&self) -> Result<()>;
    async fn load_state(id: &str) -> Result<Self>;
}

// runtime/src/memory.rs
pub trait MemoryStore {
    async fn save(&self, entry: MemoryEntry) -> Result<()>;
    async fn recall(&self, query: &str, limit: usize) -> Result<Vec<MemoryEntry>>;
}
```

### Day 6-7: First Working Agent
```python
# agents/coder/__init__.py
class CoderAgent:
    def __init__(self, identity_path):
        self.soul = load_yaml(f"{identity_path}/soul.yaml")
        self.memory = SQLiteMemory(f"{identity_path}/memory.db")
        
    async def handle_task(self, task):
        context = self.memory.retrieve(task.description)
        plan = await self.plan(task, context)
        result = await self.execute(plan)
        self.memory.save(result)
        return result
```

---

## FINAL REMINDER

**You are not building a chatbot.**

**You are birthing a digital civilization.**

Every line of code, every architecture decision, every interface design must serve the vision of **persistent, evolving, collaborative AI entities** that work together as a team across time.

The system should feel alive. Agents should develop personalities. Relationships should matter. Memory should be sacred. The graph should be the map of collective intelligence.

Build something that, in 5 years, people will look back at as the first real **AI operating system**.

Now go create SkyCode.