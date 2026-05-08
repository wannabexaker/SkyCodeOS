use ring::digest;
use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};
use thiserror::Error;
use tree_sitter::{Language, Node, Parser, TreeCursor};
use walkdir::{DirEntry, WalkDir};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GraphNode {
    pub id: String,
    pub project_id: String,
    pub kind: String, // 'file','folder','symbol','import','export'
    pub name: String,
    pub path: Option<String>,
    pub language: Option<String>,
    pub span_json: Option<String>,
    pub metadata_json: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GraphEdge {
    pub id: String,
    pub project_id: String,
    pub from_id: String,
    pub to_id: String,
    pub kind: String, // 'contains','imports','exports','depends_on','tested_by','calls'
    pub metadata_json: Option<String>,
}

#[derive(Debug, Clone)]
pub struct ScanStats {
    pub nodes_created: usize,
    pub nodes_updated: usize,
    pub edges_created: usize,
    pub files_scanned: usize,
    pub languages_found: HashMap<String, usize>,
}

#[derive(Debug, Error)]
pub enum IndexerError {
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
    #[error("database error: {0}")]
    Database(#[from] rusqlite::Error),
    #[error("invalid system time")]
    InvalidSystemTime,
    #[error("tree-sitter parser error: {0}")]
    Parser(String),
}

fn now_unix() -> Result<i64, IndexerError> {
    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|_| IndexerError::InvalidSystemTime)?
        .as_secs();
    i64::try_from(secs).map_err(|_| IndexerError::InvalidSystemTime)
}

fn detect_language(path: &Path) -> Option<String> {
    let ext = path.extension()?.to_str()?;
    match ext {
        "rs" => Some("rust".to_string()),
        "ts" | "tsx" => Some("typescript".to_string()),
        "py" => Some("python".to_string()),
        _ => None,
    }
}

fn is_ignored_dir(entry: &DirEntry) -> bool {
    if entry.depth() == 0 {
        return false;
    }

    if !entry.file_type().is_dir() {
        return false;
    }

    let name = entry.file_name().to_string_lossy();
    if name == ".git" || name == "target" || name == "node_modules" {
        return true;
    }

    name.starts_with('.')
}

fn to_unix_path(path: &Path) -> String {
    path.to_string_lossy().replace('\\', "/")
}

fn sha256_hex(input: &str) -> String {
    let hash = digest::digest(&digest::SHA256, input.as_bytes());
    let mut out = String::with_capacity(hash.as_ref().len() * 2);
    for b in hash.as_ref() {
        out.push(hex_char((b >> 4) & 0x0f));
        out.push(hex_char(b & 0x0f));
    }
    out
}

fn hex_char(value: u8) -> char {
    match value {
        0..=9 => (b'0' + value) as char,
        _ => (b'a' + (value - 10)) as char,
    }
}

fn make_file_id(project_id: &str, path: &str) -> String {
    sha256_hex(&format!("{project_id}|{path}|file"))
}

fn make_symbol_id(project_id: &str, path: &str, symbol_name: &str) -> String {
    sha256_hex(&format!("{project_id}|{path}|{symbol_name}"))
}

fn make_import_id(project_id: &str, path: &str, import_text: &str) -> String {
    sha256_hex(&format!("{project_id}|{path}|import|{import_text}"))
}

fn make_edge_id(project_id: &str, from_id: &str, to_id: &str, kind: &str) -> String {
    sha256_hex(&format!("{project_id}|{from_id}|{to_id}|{kind}"))
}

fn parse_language(language: &str) -> Result<Language, IndexerError> {
    match language {
        "rust" => Ok(tree_sitter_rust::language()),
        "typescript" => Ok(tree_sitter_typescript::language_typescript()),
        "python" => Ok(tree_sitter_python::language()),
        _ => Err(IndexerError::Parser(format!(
            "unsupported language: {language}"
        ))),
    }
}

fn node_text(source: &str, node: Node<'_>) -> Option<String> {
    node.utf8_text(source.as_bytes())
        .ok()
        .map(|s| s.to_string())
}

fn node_name(source: &str, node: Node<'_>) -> Option<String> {
    if let Some(name_node) = node.child_by_field_name("name") {
        return node_text(source, name_node);
    }

    node_text(source, node)
}

fn span_json(node: Node<'_>) -> String {
    let sp = node.start_position();
    let ep = node.end_position();
    format!(
        "{{\"start_byte\":{},\"end_byte\":{},\"start\":{{\"row\":{},\"column\":{}}},\"end\":{{\"row\":{},\"column\":{}}}}}",
        node.start_byte(),
        node.end_byte(),
        sp.row,
        sp.column,
        ep.row,
        ep.column
    )
}

fn is_symbol_kind(language: &str, kind: &str) -> bool {
    match language {
        "rust" => matches!(kind, "function_item" | "struct_item" | "enum_item"),
        "typescript" => matches!(
            kind,
            "function_declaration" | "class_declaration" | "enum_declaration"
        ),
        "python" => matches!(kind, "function_definition" | "class_definition"),
        _ => false,
    }
}

fn is_callable_symbol_kind(language: &str, kind: &str) -> bool {
    match language {
        "rust" => kind == "function_item",
        "typescript" => kind == "function_declaration",
        "python" => kind == "function_definition",
        _ => false,
    }
}

fn is_import_kind(language: &str, kind: &str) -> bool {
    match language {
        "rust" => kind == "use_declaration",
        "typescript" => kind == "import_statement",
        "python" => kind == "import_statement" || kind == "import_from_statement",
        _ => false,
    }
}

fn is_call_kind(language: &str, kind: &str) -> bool {
    match language {
        "rust" => kind == "call_expression",
        "typescript" => kind == "call_expression",
        "python" => kind == "call",
        _ => false,
    }
}

fn extract_import_ref(language: &str, raw: &str) -> Option<String> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return None;
    }

    if language == "rust" {
        // use foo::bar::Baz;
        let body = trimmed.strip_prefix("use ")?.strip_suffix(';')?.trim();
        return body.split("::").last().map(|s| s.trim().to_string());
    }

    if language == "typescript" {
        // import ... from 'module';
        if let Some(pos) = trimmed.find(" from ") {
            let frag = &trimmed[pos + 6..].trim();
            let m = frag.trim_matches(';').trim_matches('\"').trim_matches('\'');
            return m.split('/').last().map(|s| s.to_string());
        }

        // import 'module';
        if let Some(stripped) = trimmed.strip_prefix("import ") {
            let m = stripped
                .trim()
                .trim_matches(';')
                .trim_matches('\"')
                .trim_matches('\'');
            return m.split('/').last().map(|s| s.to_string());
        }
    }

    if language == "python" {
        // import pkg.mod or from pkg.mod import x
        if let Some(rest) = trimmed.strip_prefix("import ") {
            return rest.split('.').next_back().map(|s| s.trim().to_string());
        }
        if let Some(rest) = trimmed.strip_prefix("from ") {
            return rest
                .split_whitespace()
                .next()
                .and_then(|m| m.split('.').next_back())
                .map(|s| s.trim().to_string());
        }
    }

    None
}

fn collect_call_names(language: &str, source: &str, symbol_node: Node<'_>) -> Vec<String> {
    let walk_root = symbol_node
        .child_by_field_name("body")
        .unwrap_or(symbol_node);
    let mut calls = Vec::new();
    let mut seen = HashSet::new();
    let mut cursor = walk_root.walk();

    visit_call_tree(language, source, &mut cursor, &mut calls, &mut seen);

    calls
}

fn visit_call_tree(
    language: &str,
    source: &str,
    cursor: &mut TreeCursor<'_>,
    calls: &mut Vec<String>,
    seen: &mut HashSet<String>,
) {
    loop {
        let node = cursor.node();
        if is_call_kind(language, node.kind()) {
            if let Some(callee) = extract_call_callee_name(source, node) {
                if seen.insert(callee.clone()) {
                    calls.push(callee);
                }
            }
        }

        if cursor.goto_first_child() {
            visit_call_tree(language, source, cursor, calls, seen);
            let _ = cursor.goto_parent();
        }

        if !cursor.goto_next_sibling() {
            break;
        }
    }
}

fn extract_call_callee_name(source: &str, call_node: Node<'_>) -> Option<String> {
    let function_node = call_node.child_by_field_name("function")?;
    extract_callee_name(source, function_node)
}

fn extract_callee_name(source: &str, node: Node<'_>) -> Option<String> {
    match node.kind() {
        "identifier" | "field_identifier" | "property_identifier" => {
            simple_callee_text(source, node)
        }
        "field_expression" => node
            .child_by_field_name("field")
            .and_then(|n| extract_callee_name(source, n)),
        "attribute" => node
            .child_by_field_name("attribute")
            .and_then(|n| extract_callee_name(source, n)),
        "member_expression" => node
            .child_by_field_name("property")
            .and_then(|n| extract_callee_name(source, n)),
        "scoped_identifier" => node
            .child_by_field_name("name")
            .and_then(|n| extract_callee_name(source, n))
            .or_else(|| last_named_child_callee(source, node)),
        "generic_function" | "call_expression" | "call" => node
            .child_by_field_name("function")
            .and_then(|n| extract_callee_name(source, n)),
        _ => {
            for field_name in ["name", "function", "field", "attribute", "property"] {
                if let Some(name) = node
                    .child_by_field_name(field_name)
                    .and_then(|n| extract_callee_name(source, n))
                {
                    return Some(name);
                }
            }

            if node.named_child_count() == 0 {
                return simple_callee_text(source, node);
            }

            last_named_child_callee(source, node)
        }
    }
}

fn last_named_child_callee(source: &str, node: Node<'_>) -> Option<String> {
    let mut idx = node.named_child_count();
    while idx > 0 {
        idx -= 1;
        if let Some(child) = node.named_child(idx) {
            if let Some(name) = extract_callee_name(source, child) {
                return Some(name);
            }
        }
    }
    None
}

fn simple_callee_text(source: &str, node: Node<'_>) -> Option<String> {
    let text = node_text(source, node)?;
    let trimmed = text.trim();
    if trimmed.is_empty() {
        return None;
    }

    if trimmed
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || c == '_')
    {
        return Some(trimmed.to_string());
    }

    None
}

fn should_skip_incremental(
    conn: &Connection,
    file_id: &str,
    mtime: i64,
    size: i64,
) -> Result<bool, IndexerError> {
    let mut stmt =
        conn.prepare("SELECT metadata_json FROM graph_nodes WHERE id = ?1 AND kind = 'file'")?;
    let existing: Option<String> = stmt
        .query_row(params![file_id], |row| row.get(0))
        .optional()?;

    let Some(meta_json) = existing else {
        return Ok(false);
    };

    let parsed: Value = match serde_json::from_str(&meta_json) {
        Ok(v) => v,
        Err(_) => return Ok(false),
    };

    let old_mtime = parsed.get("mtime").and_then(Value::as_i64);
    let old_size = parsed.get("size").and_then(Value::as_i64);

    Ok(old_mtime == Some(mtime) && old_size == Some(size))
}

fn cleanup_file_subgraph(
    conn: &Connection,
    project_id: &str,
    file_path: &str,
    file_id: &str,
) -> Result<(), IndexerError> {
    let mut delete_edges_from_file =
        conn.prepare("DELETE FROM graph_edges WHERE project_id = ?1 AND from_id = ?2")?;
    delete_edges_from_file.execute(params![project_id, file_id])?;

    let mut symbol_ids_stmt = conn.prepare(
        "SELECT id FROM graph_nodes WHERE project_id = ?1 AND path = ?2 AND kind IN ('symbol','import')"
    )?;

    let mut rows = symbol_ids_stmt.query(params![project_id, file_path])?;
    let mut ids = Vec::new();
    while let Some(row) = rows.next()? {
        ids.push(row.get::<_, String>(0)?);
    }

    let mut delete_edges_to =
        conn.prepare("DELETE FROM graph_edges WHERE project_id = ?1 AND to_id = ?2")?;
    for id in &ids {
        delete_edges_to.execute(params![project_id, id])?;
    }

    let mut delete_nodes = conn.prepare(
        "DELETE FROM graph_nodes WHERE project_id = ?1 AND path = ?2 AND kind IN ('symbol','import')"
    )?;
    delete_nodes.execute(params![project_id, file_path])?;

    Ok(())
}

#[derive(Debug, Clone)]
struct ParsedSymbol {
    name: String,
    span_json: String,
    calls: Vec<String>,
}

#[derive(Debug, Clone)]
struct ParsedImport {
    raw: String,
    module_ref: Option<String>,
    span_json: String,
}

struct SymbolCandidate<'tree> {
    name: String,
    span_json: String,
    can_call: bool,
    node: Node<'tree>,
}

fn parse_file(
    language: &str,
    source: &str,
) -> Result<(Vec<ParsedSymbol>, Vec<ParsedImport>), IndexerError> {
    let mut parser = Parser::new();
    let lang = parse_language(language)?;
    parser
        .set_language(&lang)
        .map_err(|e| IndexerError::Parser(format!("failed set_language: {e}")))?;

    let tree = parser
        .parse(source, None)
        .ok_or_else(|| IndexerError::Parser("tree-sitter parse returned no tree".to_string()))?;

    let mut symbol_candidates = Vec::new();
    let mut imports = Vec::new();

    let mut cursor = tree.walk();
    visit_tree(
        language,
        source,
        &mut cursor,
        &mut symbol_candidates,
        &mut imports,
    );

    let mut symbols = Vec::with_capacity(symbol_candidates.len());
    for candidate in symbol_candidates {
        let calls = if candidate.can_call {
            collect_call_names(language, source, candidate.node)
        } else {
            Vec::new()
        };

        symbols.push(ParsedSymbol {
            name: candidate.name,
            span_json: candidate.span_json,
            calls,
        });
    }

    Ok((symbols, imports))
}

fn visit_tree<'tree>(
    language: &str,
    source: &str,
    cursor: &mut TreeCursor<'tree>,
    symbols: &mut Vec<SymbolCandidate<'tree>>,
    imports: &mut Vec<ParsedImport>,
) {
    loop {
        let node = cursor.node();
        let kind = node.kind();

        if is_symbol_kind(language, kind) {
            if let Some(name) = node_name(source, node) {
                if !name.trim().is_empty() {
                    symbols.push(SymbolCandidate {
                        name,
                        span_json: span_json(node),
                        can_call: is_callable_symbol_kind(language, kind),
                        node,
                    });
                }
            }
        }

        if is_import_kind(language, kind) {
            if let Some(raw) = node_text(source, node) {
                let module_ref = extract_import_ref(language, &raw);
                imports.push(ParsedImport {
                    raw,
                    module_ref,
                    span_json: span_json(node),
                });
            }
        }

        if cursor.goto_first_child() {
            visit_tree(language, source, cursor, symbols, imports);
            let _ = cursor.goto_parent();
        }

        if !cursor.goto_next_sibling() {
            break;
        }
    }
}

/// Scan a project directory and index it into graph_nodes and graph_edges.
///
/// Supports: Rust, TypeScript, Python (via tree-sitter)
/// Other languages are warned but skipped.
/// Incremental: skips files where mtime + size unchanged since last scan.
pub fn scan_project(
    conn: &Connection,
    project_id: &str,
    root: &Path,
) -> Result<ScanStats, IndexerError> {
    let now = now_unix()?;
    let mut stats = ScanStats {
        nodes_created: 0,
        nodes_updated: 0,
        edges_created: 0,
        files_scanned: 0,
        languages_found: HashMap::new(),
    };

    let mut file_records: Vec<(PathBuf, String, String, String)> = Vec::new();
    let mut file_stem_index: HashMap<String, Vec<String>> = HashMap::new();

    for entry in WalkDir::new(root)
        .into_iter()
        .filter_entry(|e| !is_ignored_dir(e))
        .filter_map(Result::ok)
    {
        if !entry.file_type().is_file() {
            continue;
        }

        let path = entry.path().to_path_buf();
        let Some(language) = detect_language(&path) else {
            continue;
        };

        let rel_path = path.strip_prefix(root).unwrap_or(&path);
        let rel_path_str = to_unix_path(rel_path);
        let file_id = make_file_id(project_id, &rel_path_str);

        if let Some(stem) = path.file_stem().and_then(|s| s.to_str()) {
            file_stem_index
                .entry(stem.to_string())
                .or_default()
                .push(file_id.clone());
        }

        file_records.push((path, rel_path_str, language, file_id));
    }

    let mut seen_file_ids = HashSet::new();
    let mut incremental_skip = HashSet::new();

    // Pass 1: upsert all file nodes so import edges can safely target them.
    for (abs_path, rel_path_str, language, file_id) in &file_records {
        if !seen_file_ids.insert(file_id.clone()) {
            continue;
        }

        *stats.languages_found.entry(language.clone()).or_insert(0) += 1;

        let meta = fs::metadata(abs_path)?;
        let mtime_secs = meta
            .modified()
            .ok()
            .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
            .and_then(|d| i64::try_from(d.as_secs()).ok())
            .unwrap_or(0);
        let size = i64::try_from(meta.len()).unwrap_or(0);

        if should_skip_incremental(conn, file_id, mtime_secs, size)? {
            incremental_skip.insert(file_id.clone());
        }

        let metadata_json = format!("{{\"mtime\":{},\"size\":{}}}", mtime_secs, size);
        let file_name = abs_path
            .file_name()
            .and_then(|n| n.to_str())
            .map(|s| s.to_string())
            .unwrap_or_else(|| rel_path_str.clone());

        let inserted_or_updated = insert_node(
            conn,
            &GraphNode {
                id: file_id.clone(),
                project_id: project_id.to_string(),
                kind: "file".to_string(),
                name: file_name,
                path: Some(rel_path_str.clone()),
                language: Some(language.clone()),
                span_json: None,
                metadata_json: Some(metadata_json),
            },
            now,
        )?;

        if inserted_or_updated {
            stats.nodes_updated += 1;
        } else {
            stats.nodes_created += 1;
        }
    }

    seen_file_ids.clear();

    // Pass 2: parse only changed files and rebuild their symbol/import subgraph.
    for (abs_path, rel_path_str, language, file_id) in file_records {
        if !seen_file_ids.insert(file_id.clone()) {
            continue;
        }

        if incremental_skip.contains(&file_id) {
            continue;
        }

        let content = fs::read_to_string(&abs_path)?;
        let (symbols, imports) = parse_file(&language, &content)?;

        cleanup_file_subgraph(conn, project_id, &rel_path_str, &file_id)?;

        let mut symbol_calls = Vec::new();

        for symbol in &symbols {
            let symbol_id = make_symbol_id(project_id, &rel_path_str, &symbol.name);
            let symbol_inserted_or_updated = insert_node(
                conn,
                &GraphNode {
                    id: symbol_id.clone(),
                    project_id: project_id.to_string(),
                    kind: "symbol".to_string(),
                    name: symbol.name.clone(),
                    path: Some(rel_path_str.clone()),
                    language: Some(language.clone()),
                    span_json: Some(symbol.span_json.clone()),
                    metadata_json: None,
                },
                now,
            )?;

            if symbol_inserted_or_updated {
                stats.nodes_updated += 1;
            } else {
                stats.nodes_created += 1;
            }

            let contains_edge = GraphEdge {
                id: make_edge_id(project_id, &file_id, &symbol_id, "contains"),
                project_id: project_id.to_string(),
                from_id: file_id.clone(),
                to_id: symbol_id.clone(),
                kind: "contains".to_string(),
                metadata_json: None,
            };
            if insert_edge(conn, &contains_edge)? {
                stats.edges_created += 1;
            }

            symbol_calls.push((symbol_id, symbol.calls.clone()));
        }

        for (caller_symbol_id, call_names) in symbol_calls {
            for callee_name in call_names {
                let Some(callee_symbol_id) =
                    find_symbol_id_by_name(conn, project_id, &rel_path_str, &callee_name)?
                else {
                    continue;
                };

                let calls_edge = GraphEdge {
                    id: make_edge_id(project_id, &caller_symbol_id, &callee_symbol_id, "calls"),
                    project_id: project_id.to_string(),
                    from_id: caller_symbol_id.clone(),
                    to_id: callee_symbol_id,
                    kind: "calls".to_string(),
                    metadata_json: None,
                };

                if insert_edge(conn, &calls_edge)? {
                    stats.edges_created += 1;
                }
            }
        }

        for import_item in imports {
            let import_id = make_import_id(project_id, &rel_path_str, &import_item.raw);
            let import_inserted_or_updated = insert_node(
                conn,
                &GraphNode {
                    id: import_id.clone(),
                    project_id: project_id.to_string(),
                    kind: "import".to_string(),
                    name: import_item.raw.clone(),
                    path: Some(rel_path_str.clone()),
                    language: Some(language.clone()),
                    span_json: Some(import_item.span_json),
                    metadata_json: None,
                },
                now,
            )?;

            if import_inserted_or_updated {
                stats.nodes_updated += 1;
            } else {
                stats.nodes_created += 1;
            }

            let contains_edge = GraphEdge {
                id: make_edge_id(project_id, &file_id, &import_id, "contains"),
                project_id: project_id.to_string(),
                from_id: file_id.clone(),
                to_id: import_id,
                kind: "contains".to_string(),
                metadata_json: None,
            };
            if insert_edge(conn, &contains_edge)? {
                stats.edges_created += 1;
            }

            if let Some(module_ref) = import_item.module_ref {
                let targets = file_stem_index
                    .get(&module_ref)
                    .cloned()
                    .unwrap_or_default();
                for target_file_id in targets {
                    if target_file_id == file_id {
                        continue;
                    }

                    let imports_edge = GraphEdge {
                        id: make_edge_id(project_id, &file_id, &target_file_id, "imports"),
                        project_id: project_id.to_string(),
                        from_id: file_id.clone(),
                        to_id: target_file_id,
                        kind: "imports".to_string(),
                        metadata_json: Some(format!(
                            "{{\"module_ref\":\"{}\"}}",
                            module_ref.replace('"', "\\\"")
                        )),
                    };

                    if insert_edge(conn, &imports_edge)? {
                        stats.edges_created += 1;
                    }
                }
            }
        }

        stats.files_scanned += 1;
    }

    Ok(stats)
}

fn find_symbol_id_by_name(
    conn: &Connection,
    project_id: &str,
    preferred_path: &str,
    name: &str,
) -> Result<Option<String>, IndexerError> {
    let mut stmt = conn.prepare(
        "SELECT id
           FROM graph_nodes
          WHERE project_id = ?1
            AND kind = 'symbol'
            AND name = ?2
          ORDER BY CASE WHEN path = ?3 THEN 0 ELSE 1 END, path
          LIMIT 1",
    )?;

    Ok(stmt
        .query_row(params![project_id, name, preferred_path], |row| row.get(0))
        .optional()?)
}

fn insert_node(conn: &Connection, node: &GraphNode, now: i64) -> Result<bool, IndexerError> {
    let mut stmt = conn.prepare(
        "INSERT INTO graph_nodes (
            id, project_id, kind, name, path, language,
            span_json, metadata_json, updated_at
        ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)
         ON CONFLICT(id) DO UPDATE SET
            project_id = excluded.project_id,
            kind = excluded.kind,
            name = excluded.name,
            path = excluded.path,
            language = excluded.language,
            span_json = excluded.span_json,
            updated_at = excluded.updated_at,
            metadata_json = excluded.metadata_json",
    )?;

    let mut exists_stmt = conn.prepare("SELECT 1 FROM graph_nodes WHERE id = ?1")?;
    let existed = exists_stmt.exists(params![node.id])?;

    stmt.execute(params![
        node.id,
        node.project_id,
        node.kind,
        node.name,
        node.path,
        node.language,
        node.span_json,
        node.metadata_json,
        now,
    ])?;

    Ok(existed)
}

fn insert_edge(conn: &Connection, edge: &GraphEdge) -> Result<bool, IndexerError> {
    let mut stmt = conn.prepare(
        "INSERT INTO graph_edges (
            id, project_id, from_id, to_id, kind, metadata_json
        ) VALUES (?1, ?2, ?3, ?4, ?5, ?6)
         ON CONFLICT(id) DO NOTHING",
    )?;

    let affected = stmt.execute(params![
        edge.id,
        edge.project_id,
        edge.from_id,
        edge.to_id,
        edge.kind,
        edge.metadata_json,
    ])?;

    Ok(affected > 0)
}
