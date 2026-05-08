# Memory System (SQLite-First)

## V1 Scopes

- Project memory
- Agent memory
- Session memory
- Decision memory
- Relationship memory (lightweight)

## Suggested Tables

- memories
- decisions
- agent_state
- tool_events
- relationships

## Retrieval Strategy

Rank by:
1. keyword relevance
2. recency
3. importance
4. scope match (project_id, agent_id)

## V1 Constraints

- No vector DB dependency.
- Add embeddings later only when measurable quality limits are reached.
