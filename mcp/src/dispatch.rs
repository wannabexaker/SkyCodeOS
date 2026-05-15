use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

use ring::signature::{Ed25519KeyPair, KeyPair};
use rusqlite::params;
use serde_json::{json, Value};
use uuid::Uuid;

use skycode_contracts::sky_loop_guard::{check_and_increment, DEFAULT_MAX_TOOL_CALLS};
use skycode_core::approval::token::ApprovalToken;
use skycode_core::approval::validator::register_signing_key;
use skycode_core::db::diff_sets::get_diff_set_members;
use skycode_inference::inference::registry::ModelRegistry;
use skycode_memory::memory::search_memories;
use skycode_tools::tools::apply::{apply_diff, apply_diff_set};
use skycode_tools::tools::diff::DiffProposal;
use skycode_tools::tools::verify::run_verify;

use crate::state::McpState;

// ============================================================================
// Helpers
// ============================================================================

fn ok_content(data: &Value) -> Value {
    json!({
        "content": [{ "type": "text", "text": data.to_string() }]
    })
}

fn err_content(msg: &str) -> Value {
    json!({
        "content": [{ "type": "text", "text": msg }],
        "isError": true
    })
}

fn check_auth(args: &Value, state: &McpState) -> bool {
    // Accept the api_key either from the tool call args (caller passes it
    // explicitly) or from the SKYCODE_API_KEY environment variable
    // (operator configured it once via claude_desktop_config.json / shell).
    //
    // The env-var path lets MCP clients like Claude Desktop avoid handling
    // a credential in chat context — they invoke mutating tools with no
    // api_key argument and the MCP server resolves it locally.
    //
    // The accepted secret must still equal the server's loaded key
    // (state.api_key, which is the contents of .skycode/api.key). Anything
    // else is rejected.
    if let Some(provided) = args["api_key"].as_str() {
        if !provided.is_empty() && provided == state.api_key.as_str() {
            return true;
        }
    }

    if let Ok(env_key) = std::env::var("SKYCODE_API_KEY") {
        if !env_key.is_empty() && env_key == state.api_key.as_str() {
            return true;
        }
    }

    false
}

fn enforce_loop_guard(args: &Value, state: &McpState) -> Result<(), String> {
    let task_id = args["task_id"].as_str().unwrap_or("mcp-default");
    let agent_id = args["agent_id"].as_str().unwrap_or("mcp");
    let conn = state
        .conn
        .lock()
        .map_err(|_| "database lock poisoned".to_string())?;

    check_and_increment(&conn, task_id, agent_id, DEFAULT_MAX_TOOL_CALLS)
        .map_err(|e| format!("loop guard blocked tool call: {e}"))
}

fn now_unix() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

/// Load an existing Ed25519 signing key from `.skycode/keys/approval_ed25519.pk8`.
/// Does NOT create a new key — key creation is the responsibility of `scos approve`.
fn load_signing_key(project_root: &Path) -> Result<Ed25519KeyPair, String> {
    let key_path = project_root
        .join(".skycode")
        .join("keys")
        .join("approval_ed25519.pk8");

    let bytes = std::fs::read(&key_path).map_err(|_| {
        "signing key not found — run `scos approve` first to generate one".to_string()
    })?;

    Ed25519KeyPair::from_pkcs8(&bytes)
        .map_err(|_| "invalid signing key format in approval_ed25519.pk8".to_string())
}

fn extract_file_path(patch_unified: &str) -> String {
    for line in patch_unified.lines() {
        if let Some(path) = line.strip_prefix("--- a/") {
            return path.to_string();
        }
    }
    "unknown".to_string()
}

// ============================================================================
// Public dispatcher
// ============================================================================

/// Dispatch a `tools/call` to the appropriate handler.
///
/// Always returns an MCP content envelope:
/// - Success: `{ "content": [{ "type": "text", "text": "<json>" }] }`
/// - Failure: `{ "content": [...], "isError": true }`
pub fn dispatch_tool(name: &str, args: Value, state: &McpState) -> Value {
    match name {
        "list_models" => tool_list_models(state),
        "get_agent_state" => tool_get_agent_state(state),
        "get_diff" => tool_get_diff(&args, state),
        "search_memory" => tool_search_memory(&args, state),
        "approve_diff" => {
            if !check_auth(&args, state) {
                return err_content("Unauthorized: invalid api_key");
            }
            if let Err(e) = enforce_loop_guard(&args, state) {
                return err_content(&e);
            }
            tool_approve_diff(&args, state)
        }
        "apply_diff" => {
            if !check_auth(&args, state) {
                return err_content("Unauthorized: invalid api_key");
            }
            if let Err(e) = enforce_loop_guard(&args, state) {
                return err_content(&e);
            }
            tool_apply_diff(&args, state)
        }
        "apply_diff_set" => {
            if !check_auth(&args, state) {
                return err_content("Unauthorized: invalid api_key");
            }
            if let Err(e) = enforce_loop_guard(&args, state) {
                return err_content(&e);
            }
            tool_apply_diff_set(&args, state)
        }
        "run_verify" => {
            if !check_auth(&args, state) {
                return err_content("Unauthorized: invalid api_key");
            }
            if let Err(e) = enforce_loop_guard(&args, state) {
                return err_content(&e);
            }
            tool_run_verify(&args, state)
        }
        _ => err_content(&format!("unknown tool: {name}")),
    }
}

// ============================================================================
// Read-only tools (no auth required)
// ============================================================================

fn tool_list_models(state: &McpState) -> Value {
    match ModelRegistry::load_from_file(&state.models_yaml_path) {
        Ok(registry) => {
            let models: Vec<Value> = registry
                .models
                .iter()
                .filter(|m| m.enabled)
                .map(|m| {
                    json!({
                        "id": m.name,
                        "runtime": format!("{:?}", m.runtime),
                        "port": m.port
                    })
                })
                .collect();
            ok_content(&json!(models))
        }
        Err(e) => err_content(&format!("failed to load models.yaml: {e}")),
    }
}

fn tool_get_agent_state(state: &McpState) -> Value {
    let conn = match state.conn.lock() {
        Ok(c) => c,
        Err(_) => return err_content("database lock poisoned"),
    };

    let result = conn.query_row(
        "SELECT agent_id, project_id, state_json, session_id, updated_at, \
                test_command, verify_timeout_secs \
           FROM agent_state \
          ORDER BY updated_at DESC \
          LIMIT 1",
        [],
        |row| {
            let agent_id: String = row.get(0)?;
            let project_id: String = row.get(1)?;
            let state_json: String = row.get(2)?;
            let session_id: Option<String> = row.get(3)?;
            let updated_at: i64 = row.get(4)?;
            let test_command: Option<String> = row.get(5)?;
            let verify_timeout_secs: i64 = row.get(6)?;

            // Parse state_json to surface the active profile name when present.
            let profile = serde_json::from_str::<Value>(&state_json)
                .ok()
                .and_then(|v| v.get("profile").and_then(Value::as_str).map(str::to_string));

            Ok(json!({
                "agent_id":            agent_id,
                "project_id":          project_id,
                "active_profile":      profile,
                "session_id":          session_id,
                "test_command":        test_command,
                "verify_timeout_secs": verify_timeout_secs,
                "updated_at":          updated_at
            }))
        },
    );

    match result {
        Ok(v) => ok_content(&v),
        Err(rusqlite::Error::QueryReturnedNoRows) => ok_content(&json!({
            "agent_id":            null,
            "project_id":          null,
            "active_profile":      null,
            "session_id":          null,
            "test_command":        null,
            "verify_timeout_secs": null,
            "updated_at":          null
        })),
        Err(e) => err_content(&format!("database error: {e}")),
    }
}

fn tool_get_diff(args: &Value, state: &McpState) -> Value {
    let diff_id = match args["diff_id"].as_str() {
        Some(id) => id,
        None => return err_content("missing required field: diff_id"),
    };

    let conn = match state.conn.lock() {
        Ok(c) => c,
        Err(_) => return err_content("database lock poisoned"),
    };

    let result = conn.query_row(
        "SELECT id, task_id, project_id, patch_unified, affected_files_json, created_at \
         FROM diff_proposals WHERE id = ?1 LIMIT 1",
        params![diff_id],
        |row| {
            let id: String = row.get(0)?;
            let task_id: Option<String> = row.get(1)?;
            let project_id: String = row.get(2)?;
            let patch_unified: String = row.get(3)?;
            let affected_files_json: String = row.get(4)?;
            let created_at: i64 = row.get(5)?;
            Ok((
                id,
                task_id,
                project_id,
                patch_unified,
                affected_files_json,
                created_at,
            ))
        },
    );

    match result {
        Ok((id, task_id, project_id, patch_unified, affected_files_json, created_at)) => {
            let affected_files: Value =
                serde_json::from_str(&affected_files_json).unwrap_or(json!([]));
            ok_content(&json!({
                "id": id,
                "task_id": task_id,
                "project_id": project_id,
                "patch_unified": patch_unified,
                "affected_files": affected_files,
                "created_at": created_at
            }))
        }
        Err(rusqlite::Error::QueryReturnedNoRows) => {
            err_content(&format!("diff not found: {diff_id}"))
        }
        Err(e) => err_content(&format!("database error: {e}")),
    }
}

fn tool_search_memory(args: &Value, state: &McpState) -> Value {
    let query = match args["query"].as_str() {
        Some(q) => q,
        None => return err_content("missing required field: query"),
    };
    let limit = args["limit"].as_u64().unwrap_or(10) as usize;

    let conn = match state.conn.lock() {
        Ok(c) => c,
        Err(_) => return err_content("database lock poisoned"),
    };

    match search_memories(&conn, query, "default", "coder-primary", "project", limit) {
        Ok(memories) => {
            let results: Vec<Value> = memories
                .iter()
                .map(|m| {
                    json!({
                        "id": m.id,
                        "content": m.content,
                        "scope": m.scope,
                        "importance": m.importance
                    })
                })
                .collect();
            ok_content(&json!(results))
        }
        Err(e) => err_content(&format!("memory search failed: {e}")),
    }
}

// ============================================================================
// Mutating tools (api_key required)
// ============================================================================

fn tool_approve_diff(args: &Value, state: &McpState) -> Value {
    let diff_id = match args["diff_id"].as_str() {
        Some(id) => id,
        None => return err_content("missing required field: diff_id"),
    };
    let agent_id = args["agent_id"].as_str().unwrap_or("coder-primary");
    let _task_id = args["task_id"].as_str().unwrap_or("mcp-task");

    // Load signing key — key must already exist (created by `scos approve`).
    let key_pair = match load_signing_key(&state.project_root) {
        Ok(k) => k,
        Err(e) => return err_content(&e),
    };

    let conn = match state.conn.lock() {
        Ok(c) => c,
        Err(_) => return err_content("database lock poisoned"),
    };

    // Resolve project_id from DB (not from caller — prevents cross-project abuse).
    let project_id: String = match conn.query_row(
        "SELECT project_id FROM diff_proposals WHERE id = ?1 LIMIT 1",
        params![diff_id],
        |row| row.get(0),
    ) {
        Ok(p) => p,
        Err(rusqlite::Error::QueryReturnedNoRows) => {
            return err_content(&format!("diff not found: {diff_id}"));
        }
        Err(e) => return err_content(&format!("database error: {e}")),
    };

    let nonce = Uuid::new_v4().to_string();
    let token = match ApprovalToken::create_signed(
        &project_id,
        diff_id,
        agent_id,
        agent_id,
        &nonce,
        &key_pair,
    ) {
        Ok(t) => t,
        Err(e) => return err_content(&format!("token creation failed: {e}")),
    };

    // Register public key in DB so validate_token can look it up.
    let pub_key_hex: String = key_pair
        .public_key()
        .as_ref()
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect();

    if let Err(e) = register_signing_key(&conn, agent_id, &pub_key_hex, now_unix()) {
        return err_content(&format!("key registration failed: {e}"));
    }

    match serde_json::to_value(&token) {
        Ok(token_val) => ok_content(&token_val),
        Err(e) => err_content(&format!("token serialization failed: {e}")),
    }
}

fn tool_apply_diff(args: &Value, state: &McpState) -> Value {
    let diff_id = match args["diff_id"].as_str() {
        Some(id) => id,
        None => return err_content("missing required field: diff_id"),
    };
    let agent_id = args["agent_id"].as_str().unwrap_or("coder-primary");
    let task_id = args["task_id"].as_str().unwrap_or("mcp-task");

    let token: ApprovalToken = match serde_json::from_value(args["token"].clone()) {
        Ok(t) => t,
        Err(e) => return err_content(&format!("invalid token: {e}")),
    };

    let conn = match state.conn.lock() {
        Ok(c) => c,
        Err(_) => return err_content("database lock poisoned"),
    };

    let (project_id, patch_unified): (String, String) = match conn.query_row(
        "SELECT project_id, patch_unified FROM diff_proposals WHERE id = ?1 LIMIT 1",
        params![diff_id],
        |row| Ok((row.get(0)?, row.get(1)?)),
    ) {
        Ok(r) => r,
        Err(rusqlite::Error::QueryReturnedNoRows) => {
            return err_content(&format!("diff not found: {diff_id}"));
        }
        Err(e) => return err_content(&format!("database error: {e}")),
    };

    let parsed_id = match Uuid::parse_str(diff_id) {
        Ok(id) => id,
        Err(_) => return err_content("invalid diff_id UUID format"),
    };

    let file_path = extract_file_path(&patch_unified);
    let diff = DiffProposal {
        id: parsed_id,
        project_id: project_id.clone(),
        diff_text: patch_unified,
        file_path,
        created_at: 0,
    };

    match apply_diff(
        &*conn,
        &token,
        agent_id,
        task_id,
        &state.project_root,
        &project_id,
        &diff,
    ) {
        Ok(()) => ok_content(&json!({ "applied": true })),
        Err(e) => err_content(&format!("apply failed: {e}")),
    }
}

fn tool_apply_diff_set(args: &Value, state: &McpState) -> Value {
    let set_id = match args["set_id"].as_str() {
        Some(id) => id,
        None => return err_content("missing required field: set_id"),
    };
    let agent_id = args["agent_id"].as_str().unwrap_or("coder-primary");
    let task_id = args["task_id"].as_str().unwrap_or("mcp-task");

    let tokens: Vec<ApprovalToken> = match serde_json::from_value(args["tokens"].clone()) {
        Ok(t) => t,
        Err(e) => return err_content(&format!("invalid tokens array: {e}")),
    };

    let conn = match state.conn.lock() {
        Ok(c) => c,
        Err(_) => return err_content("database lock poisoned"),
    };

    // Resolve diff set members.
    let members = match get_diff_set_members(&*conn, set_id) {
        Ok(m) => m,
        Err(e) => return err_content(&format!("failed to get diff set members: {e}")),
    };

    if members.is_empty() {
        return err_content(&format!("diff set has no members: {set_id}"));
    }

    // Load each diff from diff_proposals.
    let mut diffs: Vec<DiffProposal> = Vec::with_capacity(members.len());
    let mut project_id = String::new();

    for member in &members {
        let result: Result<(String, String, String), _> = conn.query_row(
            "SELECT id, project_id, patch_unified FROM diff_proposals WHERE id = ?1 LIMIT 1",
            params![member.diff_id],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        );

        match result {
            Ok((id_str, proj_id, patch)) => {
                if project_id.is_empty() {
                    project_id = proj_id.clone();
                }
                let parsed_id = match Uuid::parse_str(&id_str) {
                    Ok(id) => id,
                    Err(_) => {
                        return err_content(&format!("invalid diff UUID in set: {id_str}"));
                    }
                };
                let file_path = extract_file_path(&patch);
                diffs.push(DiffProposal {
                    id: parsed_id,
                    project_id: proj_id,
                    diff_text: patch,
                    file_path,
                    created_at: 0,
                });
            }
            Err(e) => {
                return err_content(&format!("diff {} not found: {e}", member.diff_id));
            }
        }
    }

    match apply_diff_set(
        &*conn,
        set_id,
        &tokens,
        agent_id,
        task_id,
        &state.project_root,
        &project_id,
        &diffs,
    ) {
        Ok(()) => ok_content(&json!({ "applied": true, "count": diffs.len() })),
        Err(e) => err_content(&format!("apply_diff_set failed: {e}")),
    }
}

fn tool_run_verify(args: &Value, state: &McpState) -> Value {
    let timeout_secs = args["timeout_secs"].as_u64().unwrap_or(60);

    // Get test_command from agent_state.
    let conn = match state.conn.lock() {
        Ok(c) => c,
        Err(_) => return err_content("database lock poisoned"),
    };

    let test_command: Option<String> =
        match conn.query_row("SELECT test_command FROM agent_state LIMIT 1", [], |row| {
            row.get(0)
        }) {
            Ok(cmd) => cmd,
            Err(rusqlite::Error::QueryReturnedNoRows) => None,
            Err(e) => return err_content(&format!("database error: {e}")),
        };

    // Release lock before running the test command.
    drop(conn);

    let cmd = match test_command {
        Some(c) if !c.trim().is_empty() => c,
        _ => return err_content("no test_command configured in agent_state"),
    };

    match run_verify(&state.project_root, &cmd, timeout_secs) {
        Ok(outcome) => ok_content(&json!({
            "exit_code":   outcome.exit_code,
            "stdout":      outcome.stdout_truncated,
            "stderr":      outcome.stderr_truncated,
            "timed_out":   outcome.timed_out,
            "elapsed_ms":  outcome.elapsed_ms,
            "passed":      outcome.exit_code == 0 && !outcome.timed_out
        })),
        Err(e) => err_content(&format!("run_verify failed: {e}")),
    }
}
