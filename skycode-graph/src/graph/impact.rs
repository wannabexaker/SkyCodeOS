use rusqlite::{params, Connection};
use thiserror::Error;

use super::indexer::GraphNode;

#[derive(Debug, Error)]
pub enum ImpactError {
    #[error("database error: {0}")]
    Database(#[from] rusqlite::Error),
    #[error("node not found")]
    NotFound,
}

/// Query impact of changes to a given node.
///
/// Returns all nodes that depend on the target node (transitive closure).
/// Uses the recursive CTE from docs/schemas.md:
///
/// ```sql
/// WITH RECURSIVE deps(id, depth) AS (
///   SELECT from_id, 1 FROM graph_edges
///    WHERE to_id = :target_id AND kind IN ('imports','calls','depends_on')
///   UNION
///   SELECT e.from_id, d.depth + 1
///     FROM graph_edges e JOIN deps d ON e.to_id = d.id
///    WHERE d.depth < :max_depth
/// )
/// SELECT n.* FROM graph_nodes n WHERE n.id IN (SELECT id FROM deps);
/// ```
pub fn impact_query(
    conn: &Connection,
    target_node_id: &str,
    max_depth: i32,
) -> Result<Vec<GraphNode>, ImpactError> {
    let mut stmt = conn.prepare(
        "WITH RECURSIVE deps(id, depth) AS (
           SELECT from_id, 1 FROM graph_edges
            WHERE to_id = ?1 AND kind IN ('imports', 'calls', 'depends_on')
           UNION ALL
           SELECT e.from_id, d.depth + 1
             FROM graph_edges e
             JOIN deps d ON e.to_id = d.id
            WHERE d.depth < ?2
         )
         SELECT
            n.id, n.project_id, n.kind, n.name, n.path, n.language,
            n.span_json, n.metadata_json
         FROM graph_nodes n
         WHERE n.id IN (SELECT id FROM deps)
         ORDER BY n.id",
    )?;

    let mut results = Vec::new();

    let mut rows = stmt.query(params![target_node_id, max_depth])?;

    while let Some(row) = rows.next()? {
        let node = GraphNode {
            id: row.get::<_, String>(0)?,
            project_id: row.get::<_, String>(1)?,
            kind: row.get::<_, String>(2)?,
            name: row.get::<_, String>(3)?,
            path: row.get::<_, Option<String>>(4)?,
            language: row.get::<_, Option<String>>(5)?,
            span_json: row.get::<_, Option<String>>(6)?,
            metadata_json: row.get::<_, Option<String>>(7)?,
        };
        results.push(node);
    }

    Ok(results)
}
