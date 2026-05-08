//! Context-reduction benchmark — verifies graph-guided retrieval uses ≤50%
//! of the token budget that naïve full-file loading would require.

use std::fs;
use std::path::Path;

use rusqlite::Connection;
use skycode_runtime::graph::scan_project;

// ─── helpers ─────────────────────────────────────────────────────────────────

fn apply_schema(conn: &Connection) -> Result<(), Box<dyn std::error::Error>> {
    let schema_path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("memory")
        .join("migrations")
        .join("001_initial.sql");
    let sql = fs::read_to_string(schema_path)?;
    conn.execute_batch(&sql)?;
    Ok(())
}

// ─── benchmark ───────────────────────────────────────────────────────────────

/// Compare token cost of full-file context vs graph-based reference context.
///
/// Naïve baseline   : load every .rs source file completely.
///                    Token estimate = total bytes / 4 (industry ~4 bytes/token).
///
/// Graph path        : store graph_nodes via scan_project, then synthesise
///                    refs like "graph:function:<id>" (≈40 bytes each).
///                    Token estimate = node_count × 40 / 4.
///
/// Requirement       : graph_tokens ≤ 0.50 × naive_tokens.
#[test]
fn test_graph_context_vs_naive_baseline() -> Result<(), Box<dyn std::error::Error>> {
    let conn = Connection::open_in_memory()?;
    apply_schema(&conn)?;

    let workspace_root = Path::new(env!("CARGO_MANIFEST_DIR")).join("..");

    // Scan the canonical workspace source crates into the graph.
    let _ = scan_project(&conn, "bench-ctx", &workspace_root)?;

    // ── Naïve baseline: sum all .rs file sizes ────────────────────────────
    let mut naive_bytes: u64 = 0;
    for entry in walkdir::WalkDir::new(&workspace_root)
        .follow_links(false)
        .into_iter()
        .filter_map(|e| e.ok())
        .filter(|e| {
            if e.path()
                .components()
                .any(|c| c.as_os_str() == "target" || c.as_os_str() == "target-codex")
            {
                return false;
            }

            e.file_type().is_file() && e.path().extension().map(|x| x == "rs").unwrap_or(false)
        })
    {
        naive_bytes += entry.metadata().map(|m| m.len()).unwrap_or(0);
    }

    assert!(
        naive_bytes > 0,
        "no .rs files found in workspace source crates — scan corpus is empty"
    );
    let naive_tokens = naive_bytes / 4;

    // ── Graph path: count stored nodes, estimate ref tokens ───────────────
    // Each graph ref looks like "graph:function:<40-char-hex-id>" ≈ 60 bytes,
    // but we conservatively budget 80 bytes (20 tokens) per node to account for
    // name + kind metadata that would be included in context.
    let node_count: i64 = conn.query_row(
        "SELECT COUNT(*) FROM graph_nodes WHERE project_id = 'bench-ctx'",
        [],
        |r| r.get(0),
    )?;

    assert!(
        node_count > 0,
        "scan produced no graph_nodes — corpus is empty"
    );

    // 80 bytes per node ref, 4 bytes per token
    let graph_tokens: u64 = (node_count as u64) * 80 / 4;

    let ratio = graph_tokens as f64 / naive_tokens as f64;

    println!(
        "Context reduction benchmark:\n  \
         naive  = {naive_bytes} bytes → {naive_tokens} tokens\n  \
         graph  = {node_count} nodes  → {graph_tokens} tokens\n  \
         ratio  = {ratio:.3}  (must be ≤ 0.50)"
    );

    assert!(
        ratio <= 0.50,
        "graph context ({graph_tokens} tokens) exceeds 50% of naïve baseline \
         ({naive_tokens} tokens): ratio = {ratio:.3}"
    );

    Ok(())
}
