use std::fs;
use std::path::Path;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use rusqlite::{params, Connection};
use skycode_runtime::graph::{impact_query, scan_project};
use skycode_runtime::memory::search_memories;

#[test]
fn test_graph_impact_traversal() -> Result<(), Box<dyn std::error::Error>> {
    let conn = Connection::open_in_memory()?;
    apply_phase2_schema(&conn)?;

    let now = unix_now()?;

    let a_id = "node-file-a";
    let b_id = "node-file-b";
    let c_id = "node-file-c";

    insert_file_node(&conn, a_id, "proj-1", "A.rs", now)?;
    insert_file_node(&conn, b_id, "proj-1", "B.rs", now)?;
    insert_file_node(&conn, c_id, "proj-1", "C.rs", now)?;

    insert_edge(&conn, "edge-b-imports-a", "proj-1", b_id, a_id, "imports")?;
    insert_edge(&conn, "edge-c-imports-b", "proj-1", c_id, b_id, "imports")?;

    let impacted = impact_query(&conn, a_id, 16)?;
    let impacted_ids: std::collections::HashSet<String> =
        impacted.into_iter().map(|n| n.id).collect();

    assert!(
        impacted_ids.contains(b_id),
        "expected impacted set to include B"
    );
    assert!(
        impacted_ids.contains(c_id),
        "expected impacted set to include C transitively"
    );

    Ok(())
}

#[test]
fn test_memory_retrieval_p95_under_200ms() -> Result<(), Box<dyn std::error::Error>> {
    let conn = Connection::open_in_memory()?;
    apply_phase2_schema(&conn)?;

    let base_now = unix_now()?;
    let target_project = "project-target";
    let other_project = "project-other";
    let target_agent = "coder-primary";

    let tx = conn.unchecked_transaction()?;
    let mut stmt = tx.prepare(
        "INSERT INTO memories (
            id, project_id, agent_id, scope, content, tags, importance,
            created_at, updated_at, last_access
         ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
    )?;

    for i in 0..10_000_i64 {
        let project_id = if i % 5 == 0 {
            other_project
        } else {
            target_project
        };
        let scope = match i % 4 {
            0 => "session",
            1 => "decision",
            2 => "project",
            _ => "agent",
        };

        let keyword = match i % 10 {
            0 => "alpha",
            1 => "beta",
            2 => "gamma",
            3 => "delta",
            4 => "epsilon",
            5 => "zeta",
            6 => "eta",
            7 => "theta",
            8 => "iota",
            _ => "kappa",
        };

        let content = format!("memory {} {} module refactor dependency impact", i, keyword);
        let tags = Some(format!("tag-{},{}", i % 7, keyword));
        let importance = ((i % 10) as f64) / 10.0;
        let created_at = base_now - (i % (86400 * 30));
        let updated_at = created_at + 5;
        let last_access = created_at + 10;

        stmt.execute(params![
            format!("mem-{}", i),
            project_id,
            target_agent,
            scope,
            content,
            tags,
            importance,
            created_at,
            updated_at,
            last_access,
        ])?;
    }

    drop(stmt);
    tx.commit()?;

    let query_terms = [
        "alpha", "beta", "gamma", "delta", "epsilon", "zeta", "eta", "theta", "iota", "kappa",
    ];
    let query_scopes = ["session", "decision", "project", "agent"];

    let mut durations: Vec<Duration> = Vec::with_capacity(100);
    let mut total_results = 0usize;

    for i in 0..100_usize {
        let query = query_terms[i % query_terms.len()];
        let scope = query_scopes[i % query_scopes.len()];

        let start = Instant::now();
        let results = search_memories(&conn, query, target_project, target_agent, scope, 25)?;
        let elapsed = start.elapsed();
        durations.push(elapsed);

        for row in &results {
            assert_eq!(
                row.project_id, target_project,
                "retrieval leaked other project rows"
            );
        }

        total_results += results.len();
    }

    assert!(
        total_results > 0,
        "search returned zero results across all runs"
    );

    durations.sort();
    let p95_index = ((durations.len() as f64) * 0.95).ceil() as usize - 1;
    let p95 = durations[p95_index.min(durations.len() - 1)];

    assert!(
        p95 < Duration::from_millis(200),
        "p95 latency too high: {:?} (must be < 200ms)",
        p95
    );

    Ok(())
}

#[test]
fn test_scan_persists_across_restart() -> Result<(), Box<dyn std::error::Error>> {
    let db_path = std::env::temp_dir().join(format!("skycode-phase2-gate-{}.db", unix_now()?));
    if db_path.exists() {
        fs::remove_file(&db_path)?;
    }

    {
        let conn = Connection::open(&db_path)?;
        apply_phase2_schema(&conn)?;

        let runtime_src = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
        let stats = scan_project(&conn, "phase2-project", &runtime_src)?;
        assert!(
            stats.nodes_created > 0 || stats.nodes_updated > 0,
            "scan produced no nodes"
        );

        let inserted_count: i64 = conn.query_row(
            "SELECT COUNT(*) FROM graph_nodes WHERE project_id = ?1",
            params!["phase2-project"],
            |row| row.get(0),
        )?;
        assert!(inserted_count > 0, "expected graph_nodes rows after scan");
    }

    let persisted_count: i64 = {
        let conn = Connection::open(&db_path)?;
        conn.query_row(
            "SELECT COUNT(*) FROM graph_nodes WHERE project_id = ?1",
            params!["phase2-project"],
            |row| row.get(0),
        )?
    };

    assert!(
        persisted_count > 0,
        "graph_nodes did not persist across restart"
    );

    assert_no_inference_imports()?;

    let _ = fs::remove_file(&db_path);

    Ok(())
}

fn apply_phase2_schema(conn: &Connection) -> Result<(), Box<dyn std::error::Error>> {
    let schema_path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("memory")
        .join("migrations")
        .join("001_initial.sql");

    let schema_sql = fs::read_to_string(&schema_path)?;
    conn.execute_batch(&schema_sql)?;
    Ok(())
}

fn insert_file_node(
    conn: &Connection,
    id: &str,
    project_id: &str,
    name: &str,
    updated_at: i64,
) -> Result<(), Box<dyn std::error::Error>> {
    conn.execute(
        "INSERT INTO graph_nodes (
            id, project_id, kind, name, path, language,
            span_json, metadata_json, updated_at
         ) VALUES (?1, ?2, 'file', ?3, ?4, 'rust', NULL, NULL, ?5)",
        params![id, project_id, name, name, updated_at],
    )?;
    Ok(())
}

fn insert_edge(
    conn: &Connection,
    id: &str,
    project_id: &str,
    from_id: &str,
    to_id: &str,
    kind: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    conn.execute(
        "INSERT INTO graph_edges (id, project_id, from_id, to_id, kind, metadata_json)
         VALUES (?1, ?2, ?3, ?4, ?5, NULL)",
        params![id, project_id, from_id, to_id, kind],
    )?;
    Ok(())
}

fn assert_no_inference_imports() -> Result<(), Box<dyn std::error::Error>> {
    let runtime_src = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    let memory_dir = runtime_src.join("memory");
    let graph_dir = runtime_src.join("graph");

    for dir in [&memory_dir, &graph_dir] {
        for entry in fs::read_dir(dir)? {
            let entry = entry?;
            let path = entry.path();

            if path.extension().and_then(|e| e.to_str()) != Some("rs") {
                continue;
            }

            let content = fs::read_to_string(&path)?;
            let lower = content.to_lowercase();

            assert!(
                !lower.contains("skycode-inference"),
                "found forbidden crate reference in {}",
                path.display()
            );

            for line in content.lines() {
                let line_lower = line.to_lowercase();
                if line_lower.trim_start().starts_with("use ") && line_lower.contains("inference") {
                    panic!(
                        "found forbidden inference import in {}: {}",
                        path.display(),
                        line
                    );
                }
            }
        }
    }

    Ok(())
}

fn unix_now() -> Result<i64, Box<dyn std::error::Error>> {
    let secs = SystemTime::now().duration_since(UNIX_EPOCH)?.as_secs();
    Ok(i64::try_from(secs)?)
}
