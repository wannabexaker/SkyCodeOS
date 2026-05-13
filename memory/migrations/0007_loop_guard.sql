CREATE TABLE IF NOT EXISTS task_loop_counters (
    task_id TEXT NOT NULL,
    agent_id TEXT NOT NULL,
    tool_calls INTEGER NOT NULL DEFAULT 0,
    last_call_at INTEGER NOT NULL,
    PRIMARY KEY (task_id, agent_id)
);
