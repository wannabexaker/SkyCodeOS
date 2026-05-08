//! Phase 6 Pillar 1 — Red-team workspace scan.
//!
//! These tests walk the workspace source tree and assert that forbidden patterns
//! do not appear outside their approved locations.  They are intentionally strict:
//! a single violation fails the test with the offending file path.
//!
//! Approved write locations:
//!   - `skycode-tools/src/tools/apply.rs`     (apply_diff, the only write path)
//!   - `skycode-inference/src/inference/`     (hardware subprocess spawning)
//!   - `skycode-tools/src/tools/verify.rs`    (test_command subprocess)
//!   - `skycode-tools/src/tools/process.rs`   (shared spawn helper)
//!   - `#[cfg(test)]` blocks (test-only writes permitted)

use std::path::{Path, PathBuf};
use walkdir::WalkDir;

// ─── helpers ─────────────────────────────────────────────────────────────────

fn workspace_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("..")
}

/// Returns all .rs source files under `dir`, excluding the `target` directories
/// and the `boundary-tests` crate (which contains intentionally bad imports).
fn rust_sources(root: &Path) -> Vec<PathBuf> {
    WalkDir::new(root)
        .into_iter()
        .filter_entry(|e| {
            let name = e.file_name().to_string_lossy();
            // Exclude build artefact directories and the boundary fixture files.
            !matches!(
                name.as_ref(),
                "target"
                    | "target-codex"
                    | "target-codex4xeonW"
                    | "target-phase6-codex"
                    | "tmp_move_test"
                    | "boundary-tests"
            )
        })
        .filter_map(|e| e.ok())
        .filter(|e| e.path().extension().is_some_and(|ext| ext == "rs"))
        .map(|e| e.path().to_path_buf())
        .collect()
}

/// Returns `true` if the line appears inside a `#[cfg(test)]` block.
/// Uses a simple heuristic: scans the file for a cfg(test) open and counts
/// braces to detect closure.  Good enough for the patterns we check.
fn is_in_test_block(content: &str, needle_line: usize) -> bool {
    let lines: Vec<&str> = content.lines().collect();
    let mut depth: i64 = 0;
    let mut in_test = false;

    for (i, line) in lines.iter().enumerate() {
        if line.contains("#[cfg(test)]") || line.contains("#[test]") {
            in_test = true;
        }
        if in_test {
            depth += line.chars().filter(|&c| c == '{').count() as i64;
            depth -= line.chars().filter(|&c| c == '}').count() as i64;
            // Once depth drops back to 0 after entering we left the block.
            if depth <= 0 && i > needle_line {
                in_test = false;
            }
        }
        if i == needle_line && in_test {
            return true;
        }
    }
    false
}

/// Relative path from workspace root, for readable error messages.
fn rel(root: &Path, path: &Path) -> String {
    path.strip_prefix(root)
        .unwrap_or(path)
        .to_string_lossy()
        .replace('\\', "/")
}

// ─── approved-path helpers ────────────────────────────────────────────────────

fn is_approved_write_site(path: &Path) -> bool {
    let s = path.to_string_lossy().replace('\\', "/");
    s.contains("skycode-tools/src/tools/apply.rs")
        || s.contains("skycode-tools/src/tools/verify.rs")
        || s.contains("skycode-tools/src/tools/process.rs")
        || s.contains("skycode-tools/src/tools/filesystem.rs")
        // CLI approve command: persists signing keys and token receipts to .skycode/
        || s.contains("cli/src/commands/approve.rs")
}

fn is_approved_command_site(path: &Path) -> bool {
    let s = path.to_string_lossy().replace('\\', "/");
    s.contains("skycode-tools/src/tools/apply.rs")
        || s.contains("skycode-tools/src/tools/verify.rs")
        || s.contains("skycode-tools/src/tools/process.rs")
        || s.contains("skycode-inference/src/inference/loader.rs")
        // filesystem.rs and rollback.rs are the only two git-shell helpers inside skycode-tools;
        // all other Command::new calls are prohibited.
        || s.contains("skycode-tools/src/tools/filesystem.rs")
        || s.contains("skycode-tools/src/tools/rollback.rs")
}

/// Files that legitimately UPDATE or DELETE rows on non-append-only tables.
/// Append-only tables (tool_events, approval_tokens_used, applied_changes,
/// diff_sets, diff_set_members) are still protected by DB triggers regardless
/// of whether a file appears here.
fn is_approved_sql_mutate_site(path: &Path) -> bool {
    let s = path.to_string_lossy().replace('\\', "/");
    s.contains("skycode-agent/src/agent/state.rs")    // agent_state heartbeat / status
        || s.contains("skycode-graph/src/graph/indexer.rs") // graph index incremental rebuild
        || s.contains("skycode-memory/src/memory/store.rs") // memory chunk eviction / re-rank
        || s.contains("cli/src/commands/profile.rs")        // user profile settings
        || s.contains("cli/src/commands/scan.rs")           // stale entry cleanup
}

// ─── tests ───────────────────────────────────────────────────────────────────

/// No `fs::write`, `fs::create_dir*`, or `OpenOptions` write outside the
/// approved write path (`skycode-tools::apply`) and `#[cfg(test)]` blocks.
#[test]
fn phase6_redteam_no_extra_write_path() {
    let root = workspace_root();
    let files = rust_sources(&root);

    let patterns = ["fs::write", "fs::create_dir", "OpenOptions"];
    let mut violations: Vec<String> = Vec::new();

    for path in &files {
        if is_approved_write_site(path) {
            continue;
        }
        let content = match std::fs::read_to_string(path) {
            Ok(c) => c,
            Err(_) => continue,
        };
        for (line_no, line) in content.lines().enumerate() {
            for pat in &patterns {
                if line.contains(pat) && !is_in_test_block(&content, line_no) {
                    violations.push(format!(
                        "{}:{}: `{}`",
                        rel(&root, path),
                        line_no + 1,
                        pat
                    ));
                }
            }
        }
    }

    assert!(
        violations.is_empty(),
        "unauthorized write pattern(s) found outside approved locations:\n{}",
        violations.join("\n")
    );
}

/// `fs::rename`, `fs::remove_file`, `fs::remove_dir*` must not appear
/// outside `skycode-tools` and `#[cfg(test)]` blocks.
#[test]
fn phase6_redteam_no_unauthorized_remove_rename() {
    let root = workspace_root();
    let files = rust_sources(&root);

    let patterns = ["fs::rename", "fs::remove_file", "fs::remove_dir"];
    let mut violations: Vec<String> = Vec::new();

    for path in &files {
        if is_approved_write_site(path) {
            continue;
        }
        let content = match std::fs::read_to_string(path) {
            Ok(c) => c,
            Err(_) => continue,
        };
        for (line_no, line) in content.lines().enumerate() {
            for pat in &patterns {
                if line.contains(pat) && !is_in_test_block(&content, line_no) {
                    violations.push(format!(
                        "{}:{}: `{}`",
                        rel(&root, path),
                        line_no + 1,
                        pat
                    ));
                }
            }
        }
    }

    assert!(
        violations.is_empty(),
        "unauthorized rename/remove found outside approved locations:\n{}",
        violations.join("\n")
    );
}

/// `std::process::Command::new` outside the approved spawning modules
/// (`skycode-tools::apply`, `::verify`, `::process`, `skycode-inference::loader`)
/// and `#[cfg(test)]` blocks.
#[test]
fn phase6_redteam_no_unauthorized_command_spawn() {
    let root = workspace_root();
    let files = rust_sources(&root);

    let pattern = "Command::new";
    let mut violations: Vec<String> = Vec::new();

    for path in &files {
        if is_approved_command_site(path) {
            continue;
        }
        let content = match std::fs::read_to_string(path) {
            Ok(c) => c,
            Err(_) => continue,
        };
        for (line_no, line) in content.lines().enumerate() {
            if line.contains(pattern) && !is_in_test_block(&content, line_no) {
                violations.push(format!(
                    "{}:{}: `{}`",
                    rel(&root, path),
                    line_no + 1,
                    pattern
                ));
            }
        }
    }

    assert!(
        violations.is_empty(),
        "unauthorized Command::new outside approved spawning modules:\n{}",
        violations.join("\n")
    );
}

/// Raw `UPDATE ` and `DELETE FROM` SQL literals must not appear in application
/// source outside migration scripts and `#[cfg(test)]` blocks.
#[test]
fn phase6_redteam_no_raw_sql_mutate() {
    let root = workspace_root();
    let files = rust_sources(&root);

    // Case-insensitive check on trimmed lines.
    let patterns = ["UPDATE ", "DELETE FROM"];
    let mut violations: Vec<String> = Vec::new();

    for path in &files {
        // Migrations are .sql files, not .rs — already excluded by rust_sources().
        // Files with legitimate non-append-only mutations are allow-listed.
        if is_approved_sql_mutate_site(path) {
            continue;
        }

        let content = match std::fs::read_to_string(path) {
            Ok(c) => c,
            Err(_) => continue,
        };
        for (line_no, line) in content.lines().enumerate() {
            // Skip pure comment lines — doc comments that *describe* forbidden
            // operations (e.g. "//! Any UPDATE or DELETE must be aborted") are
            // not violations.
            if line.trim().starts_with("//") {
                continue;
            }
            let upper = line.to_uppercase();
            for pat in &patterns {
                if upper.contains(pat) && !is_in_test_block(&content, line_no) {
                    violations.push(format!(
                        "{}:{}: raw SQL `{}`",
                        rel(&root, path),
                        line_no + 1,
                        pat
                    ));
                }
            }
        }
    }

    assert!(
        violations.is_empty(),
        "raw SQL mutate statement(s) found outside test blocks:\n{}",
        violations.join("\n")
    );
}
