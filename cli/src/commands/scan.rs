use std::path::PathBuf;

use clap::Args;
use rusqlite::Connection;

use skycode_orchestrator::db::migrations::run_migrations;
use skycode_orchestrator::graph::{impact_query, scan_project};

#[derive(Debug, Args)]
pub struct ScanArgs {
    /// Path to scan.
    pub path: PathBuf,

    /// Force full rescan (clear and rebuild).
    #[arg(long)]
    pub force: bool,
}

#[derive(Debug, Args)]
pub struct GraphImpactArgs {
    /// Symbol or node ID to query impact for.
    pub symbol: String,

    /// Project ID.
    #[arg(long, default_value = "default")]
    pub project_id: String,

    /// Maximum depth for recursive query.
    #[arg(long, default_value = "5")]
    pub max_depth: i32,
}

pub fn run_scan(args: &ScanArgs) -> Result<(), Box<dyn std::error::Error>> {
    let db_path = std::env::current_dir()?.join("skycode.db");
    let conn = Connection::open(&db_path)?;

    let migrations_dir = std::env::current_dir()?.join("memory").join("migrations");

    if migrations_dir.exists() {
        run_migrations(&conn, &migrations_dir)?;
    } else {
        eprintln!(
            "warning: migrations directory not found at {:?}",
            migrations_dir
        );
    }

    if args.force {
        conn.execute_batch(
            "DELETE FROM graph_edges WHERE project_id = 'default';
             DELETE FROM graph_nodes WHERE project_id = 'default';",
        )?;
        eprintln!("force: cleared graph for project 'default'");
    }

    let stats = scan_project(&conn, "default", &args.path)?;

    println!("Scan complete:");
    println!("  Files scanned: {}", stats.files_scanned);
    println!("  Nodes created: {}", stats.nodes_created);
    println!("  Nodes updated: {}", stats.nodes_updated);
    println!("  Edges created: {}", stats.edges_created);
    println!("  Languages:");
    for (lang, count) in stats.languages_found.iter() {
        println!("    {}: {}", lang, count);
    }

    Ok(())
}

pub fn run_graph_impact(args: &GraphImpactArgs) -> Result<(), Box<dyn std::error::Error>> {
    let db_path = std::env::current_dir()?.join("skycode.db");
    let conn = Connection::open(&db_path)?;

    let migrations_dir = std::env::current_dir()?.join("memory").join("migrations");

    if migrations_dir.exists() {
        run_migrations(&conn, &migrations_dir)?;
    }

    // Resolve symbol name → node id.
    // Priority: symbol kind first, then any kind, then treat arg as raw node id.
    let node_id: String = {
        let mut sym_stmt =
            conn.prepare("SELECT id FROM graph_nodes WHERE name = ?1 AND kind = 'symbol' LIMIT 1")?;
        let sym_result =
            sym_stmt.query_row(rusqlite::params![args.symbol], |r| r.get::<_, String>(0));
        match sym_result {
            Ok(id) => id,
            Err(rusqlite::Error::QueryReturnedNoRows) => {
                // Fall back to any kind (file, import, etc.)
                let mut any_stmt =
                    conn.prepare("SELECT id FROM graph_nodes WHERE name = ?1 LIMIT 1")?;
                match any_stmt.query_row(rusqlite::params![args.symbol], |r| r.get::<_, String>(0))
                {
                    Ok(id) => id,
                    Err(rusqlite::Error::QueryReturnedNoRows) => args.symbol.clone(),
                    Err(e) => return Err(e.into()),
                }
            }
            Err(e) => return Err(e.into()),
        }
    };

    let nodes = impact_query(&conn, &node_id, args.max_depth)?;

    if nodes.is_empty() {
        println!("No nodes depend on '{}'", args.symbol);
        return Ok(());
    }

    println!("Nodes affected by changes to '{}':", args.symbol);
    for node in nodes {
        println!(
            "  {} [{}] ({})",
            node.name,
            node.kind,
            node.language.as_deref().unwrap_or("unknown")
        );
        if let Some(path) = node.path {
            println!("    {}", path);
        }
    }

    Ok(())
}
