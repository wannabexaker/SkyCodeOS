use rusqlite::{params, Connection};
use std::time::{SystemTime, UNIX_EPOCH};
use thiserror::Error;

use super::store::Memory;

#[allow(dead_code)]
const RECENCY_TAU_DAYS: f64 = 14.0;

#[derive(Debug, Error)]
pub enum RetrievalError {
    #[error("database error: {0}")]
    Database(#[from] rusqlite::Error),
    #[error("invalid system time")]
    InvalidSystemTime,
}

fn now_unix() -> Result<i64, RetrievalError> {
    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|_| RetrievalError::InvalidSystemTime)?
        .as_secs();
    i64::try_from(secs).map_err(|_| RetrievalError::InvalidSystemTime)
}

#[allow(dead_code)]
fn recency_decay(last_access_unix: i64, now_unix: i64) -> f64 {
    let seconds_elapsed = (now_unix - last_access_unix).max(0) as f64;
    let days_elapsed = seconds_elapsed / 86400.0;
    (-days_elapsed / RECENCY_TAU_DAYS).exp()
}

#[allow(dead_code)]
fn scope_match(query_scope: &str, memory_scope: &str) -> f64 {
    if query_scope == memory_scope {
        return 1.0;
    }

    // Compatible scopes get partial match
    match (query_scope, memory_scope) {
        ("session", "agent") | ("agent", "session") => 0.5,
        ("decision", "session") | ("session", "decision") => 0.5,
        _ => 0.0,
    }
}

/// Search memories with hybrid BM25 + recency + importance + scope ranking.
///
/// Returns memories ranked by:
/// score = bm25 * recency_decay * importance * scope_match
///
/// # Arguments
/// - `project_id`: filter to this project only
/// - `agent_id`: filter to this agent only  
/// - `query`: search text (searched against FTS5 index)
/// - `query_scope`: scope for scope_match calculation ('project','agent','session','decision')
/// - `limit`: max results to return
pub fn search_memories(
    conn: &Connection,
    query: &str,
    project_id: &str,
    agent_id: &str,
    query_scope: &str,
    limit: usize,
) -> Result<Vec<Memory>, RetrievalError> {
    let now = now_unix()?;

    // FTS5 rejects special chars like `/`, `.`, `(`, `:` — keep only word chars and spaces.
    let safe_query: String = query
        .chars()
        .map(|c| {
            if c.is_alphanumeric() || c == ' ' {
                c
            } else {
                ' '
            }
        })
        .collect();
    let safe_query = safe_query.trim();

    // Empty query after sanitization — return no results rather than hitting FTS5 with "".
    if safe_query.is_empty() {
        return Ok(Vec::new());
    }

    // Use FTS5's bm25() function for native ranking.
    // Combine with recency_decay, importance, and scope_match for final score.
    let mut stmt = conn.prepare(
        "SELECT
            m.id, m.project_id, m.agent_id, m.scope, m.content, m.tags, m.importance,
                        (
                            (-bm25(memories_fts, 10.0, 5.0)) *
              (1.0 / (1.0 + CAST(?5 - COALESCE(m.last_access, m.created_at) AS REAL) / 86400.0)) *
              m.importance *
              CASE
                WHEN m.scope = ?4 THEN 1.0
                WHEN (m.scope = 'session' AND ?4 = 'agent') OR (m.scope = 'agent' AND ?4 = 'session') THEN 0.5
                WHEN (m.scope = 'decision' AND ?4 = 'session') OR (m.scope = 'session' AND ?4 = 'decision') THEN 0.5
                ELSE 0.0
              END
            ) AS score
         FROM memories m
         JOIN memories_fts f ON m.rowid = f.rowid
         WHERE f.memories_fts MATCH ?1
           AND m.project_id = ?2
           AND m.agent_id = ?3
         ORDER BY score DESC
         LIMIT ?6"
    )?;

    let mut results = Vec::new();

    let mut rows = stmt.query(params![
        safe_query,
        project_id,
        agent_id,
        query_scope,
        now,
        limit as i32,
    ])?;

    while let Some(row) = rows.next()? {
        let mem = Memory {
            id: row.get(0)?,
            project_id: row.get(1)?,
            agent_id: row.get(2)?,
            scope: row.get(3)?,
            content: row.get(4)?,
            tags: row.get(5)?,
            importance: row.get(6)?,
        };
        results.push(mem);
    }

    Ok(results)
}
