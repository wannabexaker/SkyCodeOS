use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use rusqlite::{params, Connection, OptionalExtension};
use serde_json::Value;
use thiserror::Error;

use skycode_agent::agent::identity::{load_coder_primary_identity, IdentityError};
use skycode_agent::agent::intent::{build_intent, AgentIntent};
use skycode_agent::agent::profile::{load_profile, AgentProfile};
use skycode_core::skycore::{
    strip_provider_fields, ModelPolicy, SkyCoreArtifact, SkyCoreConstraints, SkyCoreRequest,
    SkyCoreResponse,
};
use skycode_graph::graph::impact_query;
use skycode_inference::inference::{
    launch_server, resolve_gpu_layers, resolve_tensor_split, InferenceError, ModelLaunchOptions,
    ModelRegistryError, ModelRegistryWatcher, ModelRuntime, SamplingExtras,
};
use skycode_memory::memory::{search_memories, RetrievalError};
use skycode_tools::tools::diff::{create_diff, DiffError, DiffProposal};

use crate::orchestrator::policy::{enforce_permission_set, PolicyError};
use crate::orchestrator::router::{
    classify_task, map_to_model, record_model_selection, RouterError, TaskClass,
};

use diffy::create_patch;

#[derive(Debug, Clone)]
pub struct TaskLoopInput {
    pub task_id: String,
    pub project_id: String,
    pub goal: String,
    pub repo_root: String,
    pub profile: String,
}

#[derive(Debug, Clone)]
pub struct TaskLoopOutput {
    pub diff: DiffProposal,
    pub response_summary: String,
}

#[derive(Debug, Error)]
pub enum OrchestratorError {
    #[error("identity error: {0}")]
    Identity(#[from] IdentityError),
    #[error("database error: {0}")]
    Database(#[from] rusqlite::Error),
    #[error("model registry error: {0}")]
    Registry(#[from] ModelRegistryError),
    #[error("diff error: {0}")]
    Diff(#[from] DiffError),
    #[error("inference error: {0}")]
    Inference(#[from] InferenceError),
    #[error("invalid system time")]
    InvalidSystemTime,
    #[error("failed to parse skycore response: {0}")]
    SkyCoreParse(#[from] skycode_core::skycore::BoundaryError),
    #[error("failed to parse or serialize JSON: {0}")]
    Json(#[from] serde_json::Error),
    #[error("memory retrieval error: {0}")]
    Retrieval(#[from] RetrievalError),
    #[error("router error: {0}")]
    Router(#[from] RouterError),
    #[error("policy error: {0}")]
    Policy(#[from] PolicyError),
    #[error("missing diff artifact in model response")]
    MissingDiffArtifact,
    #[error("model runtime must be local_gguf, got openai_compatible")]
    InvalidModelRuntime,
    #[error("model process produced no output")]
    EmptyModelOutput,
    #[error("model output invalid: {0}")]
    ModelOutputInvalid(String),
}

pub fn run_task_loop(
    conn: &Connection,
    input: &TaskLoopInput,
) -> Result<TaskLoopOutput, OrchestratorError> {
    let task_class = classify_task(&input.goal);

    let repo_root = PathBuf::from(&input.repo_root);
    let agents_root = repo_root.join("agents");

    // 1) load identity
    let identity = load_coder_primary_identity(&agents_root)?;

    // 2) build intent
    let intent = build_intent(&identity, &input.goal);

    // Resolve active profile: explicit CLI arg > saved agent_state > default.
    let active_profile = resolve_active_profile(conn, input);
    let mut profile = load_profile(&agents_root, &active_profile).unwrap_or_else(|e| {
        eprintln!("warning: could not load profile '{active_profile}': {e} - using defaults");
        AgentProfile {
            name: active_profile.clone(),
            ..Default::default()
        }
    });
    apply_profile_state_overrides(conn, input, &mut profile)?;
    enforce_permission_set(&profile.permissions, "file_write")?;

    // 3) resolve context refs from memory + graph
    let context_refs = resolve_context_refs(
        conn,
        &input.project_id,
        &identity.id,
        &input.goal,
        profile.context_chunks,
    )?;

    // 4) build SkyCore request
    let request = build_skycore_request(
        &input.task_id,
        &identity.id,
        &intent,
        context_refs,
        &profile,
    );

    // 5) invoke model via registry loader + parse response
    let response =
        invoke_model_and_parse_response(conn, &repo_root, &request, task_class, &profile)?;

    // 6) store diff proposal + return to CLI
    let artifact = select_diff_artifact(&response)?;

    let file_path = artifact
        .affected_files
        .as_ref()
        .and_then(|v| v.first().cloned())
        .unwrap_or_else(|| "README.md".to_string());

    let mut diff = create_diff(&input.project_id, Path::new(&file_path), "", "")?;

    // Prefer new_content (full-file rewrite) — compute the diff ourselves so it
    // is always correct regardless of model quality.
    if let Some(new_content) = &artifact.new_content {
        let old_content = std::fs::read_to_string(repo_root.join(&file_path))
            .unwrap_or_default()
            .replace("\r\n", "\n");
        let git_patch = build_rewrite_patch(&file_path, &old_content, new_content);
        if !git_patch.trim().is_empty() {
            diff.diff_text = git_patch;
        }
    } else {
        let patch = artifact.patch_unified.clone().unwrap_or_default();
        if !patch.is_empty() {
            let patch = fix_diff_prefixes(&patch);
            // git apply requires --- / +++ header lines; prepend them if the model omitted them.
            diff.diff_text = if patch.trim_start().starts_with("@@") {
                format!("--- a/{file_path}\n+++ b/{file_path}\n{patch}")
            } else {
                patch
            };
        }
    }

    store_diff_proposal(
        conn,
        &input.task_id,
        &input.project_id,
        &identity.id,
        &diff,
        &artifact.affected_files,
    )?;

    Ok(TaskLoopOutput {
        diff,
        response_summary: response.summary,
    })
}

pub fn select_diff_artifact(
    response: &SkyCoreResponse,
) -> Result<SkyCoreArtifact, OrchestratorError> {
    response
        .artifacts
        .iter()
        .find(|a| is_diff_artifact_kind(&a.kind))
        .map(normalize_diff_artifact)
        .ok_or(OrchestratorError::MissingDiffArtifact)
}

fn is_diff_artifact_kind(kind: &str) -> bool {
    matches!(kind, "diff" | "rewrite" | "file" | "create")
}

fn normalize_diff_artifact(artifact: &SkyCoreArtifact) -> SkyCoreArtifact {
    let mut normalized = artifact.clone();

    if matches!(artifact.kind.as_str(), "file" | "create") {
        normalized.kind = "rewrite".to_string();

        if normalized.new_content.is_none() {
            normalized.new_content = normalized.content.clone();
        }

        let needs_affected_files = normalized
            .affected_files
            .as_ref()
            .map(|files| files.is_empty())
            .unwrap_or(true);

        if needs_affected_files && !normalized.id.trim().is_empty() {
            normalized.affected_files = Some(vec![normalized.id.clone()]);
        }
    }

    normalized
}

pub fn build_rewrite_patch(file_path: &str, old_content: &str, new_content: &str) -> String {
    let old_normalized = old_content.replace("\r\n", "\n");
    let new_normalized = new_content.replace("\r\n", "\n");
    let patch_obj = create_patch(&old_normalized, &new_normalized);
    let patch_str = format!("{patch_obj}");

    if patch_str.trim().is_empty() {
        return String::new();
    }

    let body = diffy_patch_body(&patch_str);

    if old_normalized.is_empty() && !new_normalized.is_empty() {
        return format!(
            "diff --git a/{file_path} b/{file_path}\n\
new file mode 100644\n\
--- /dev/null\n\
+++ b/{file_path}\n\
{body}"
        );
    }

    if !old_normalized.is_empty() && new_normalized.is_empty() {
        return format!(
            "diff --git a/{file_path} b/{file_path}\n\
deleted file mode 100644\n\
--- a/{file_path}\n\
+++ /dev/null\n\
{body}"
        );
    }

    if patch_str.starts_with("--- ") {
        return format!(
            "diff --git a/{file_path} b/{file_path}\n\
--- a/{file_path}\n\
+++ b/{file_path}\n\
{body}"
        );
    }

    patch_str
}

fn diffy_patch_body(patch_str: &str) -> String {
    patch_str
        .splitn(3, '\n')
        .nth(2)
        .unwrap_or(patch_str)
        .to_string()
}

fn resolve_active_profile(conn: &Connection, input: &TaskLoopInput) -> String {
    if input.profile != "precise" {
        return input.profile.clone();
    }

    let saved: Option<String> = conn
        .query_row(
            "SELECT json_extract(state_json, '$.profile') FROM agent_state
              WHERE agent_id = 'coder-primary' AND project_id = ?1",
            params![input.project_id],
            |r| r.get(0),
        )
        .optional()
        .unwrap_or(None)
        .flatten();

    saved.unwrap_or_else(|| "precise".to_string())
}

fn apply_profile_state_overrides(
    conn: &Connection,
    input: &TaskLoopInput,
    profile: &mut AgentProfile,
) -> Result<(), OrchestratorError> {
    let state_json: Option<String> = conn
        .query_row(
            "SELECT state_json FROM agent_state
              WHERE agent_id = 'coder-primary' AND project_id = ?1",
            params![input.project_id],
            |r| r.get(0),
        )
        .optional()?;

    let Some(state_json) = state_json else {
        return Ok(());
    };
    let value = serde_json::from_str::<Value>(&state_json)?;

    if let Some(temp) = value.get("temperature").and_then(Value::as_f64) {
        if temp.is_finite() {
            profile.temperature = temp as f32;
        }
    }
    if let Some(repeat_penalty) = value.get("repeat_penalty").and_then(Value::as_f64) {
        if repeat_penalty.is_finite() {
            profile.repeat_penalty = repeat_penalty as f32;
        }
    }
    if let Some(model) = value.get("model").and_then(Value::as_str) {
        if !model.trim().is_empty() {
            profile.model = model.to_string();
        }
    }

    Ok(())
}

fn resolve_context_refs(
    conn: &Connection,
    project_id: &str,
    agent_id: &str,
    goal: &str,
    context_chunks: usize,
) -> Result<Vec<String>, OrchestratorError> {
    let memories = search_memories(conn, goal, project_id, agent_id, "project", context_chunks)?;

    let mut refs = Vec::new();
    for m in memories {
        refs.push(format!("memory:{}", m.id));
    }

    // Use goal as a possible node id seed; impact query may return empty and that's fine.
    if let Ok(impacts) = impact_query(conn, goal, 2) {
        for n in impacts {
            refs.push(format!("graph:{}:{}", n.kind, n.id));
        }
    }

    refs.truncate(context_chunks);
    Ok(refs)
}

fn build_skycore_request(
    task_id: &str,
    agent_id: &str,
    intent: &AgentIntent,
    context_refs: Vec<String>,
    profile: &AgentProfile,
) -> SkyCoreRequest {
    let max_output_tokens = match i32::try_from(profile.max_tokens) {
        Ok(value) => value,
        Err(_) => i32::MAX,
    };

    SkyCoreRequest {
        skycore_version: "0.1".to_string(),
        task_id: task_id.to_string(),
        agent_id: agent_id.to_string(),
        goal: intent.goal.clone(),
        context_refs,
        tools_allowed: intent.requested_tools.clone(),
        model_policy: ModelPolicy {
            preferred: profile.model.clone(),
            fallback: "local-fallback".to_string(),
            profile: profile.name.clone(),
        },
        output_contract: intent.output_contract.clone(),
        constraints: SkyCoreConstraints {
            max_output_tokens,
            stream: Some(true),
            stop: Some(Vec::new()),
        },
    }
}

fn invoke_model_and_parse_response(
    conn: &Connection,
    repo_root: &Path,
    request: &SkyCoreRequest,
    task_class: TaskClass,
    profile: &AgentProfile,
) -> Result<SkyCoreResponse, OrchestratorError> {
    // Mock intercept (used only in integration tests).
    // If .skycode/mock_model_response.json exists in the repo root, use its
    // contents as the model response and skip launching llama.cpp.
    let mock_path = repo_root.join(".skycode").join("mock_model_response.json");
    if mock_path.exists() {
        let json_text = std::fs::read_to_string(&mock_path).map_err(|e| {
            OrchestratorError::ModelOutputInvalid(format!("mock response read error: {e}"))
        })?;
        let raw: serde_json::Value = serde_json::from_str(&json_text)?;
        let response = strip_provider_fields(raw)?;
        return Ok(response);
    }

    let models_path = repo_root.join("agents").join("models.yaml");
    let mut watcher = ModelRegistryWatcher::load(models_path)?;
    let _ = watcher.reload_if_changed()?;

    let model = match watcher.model(&profile.model) {
        Ok(model) => model,
        Err(_) => map_to_model(task_class, watcher.registry())?.clone(),
    };
    if let Err(err) = record_model_selection(
        conn,
        &request.task_id,
        &model.name,
        &request.model_policy.profile,
    ) {
        eprintln!("warning: failed to record model selection: {err}");
    }

    if model.runtime != ModelRuntime::LocalGguf {
        return Err(OrchestratorError::InvalidModelRuntime);
    }

    let launch = ModelLaunchOptions {
        executable: model.executable.clone(),
        model_path: Path::new(&model.path).to_path_buf(),
        ctx_size: model.ctx_size,
        threads: model.threads,
        n_gpu_layers: resolve_gpu_layers(
            &model.gpu_layers,
            Path::new(&model.path),
            &model.vram_budget_mb,
        ),
        n_cpu_moe: model.n_cpu_moe,
        prompt: None,
        temp: profile.temperature,
        repeat_penalty: profile.repeat_penalty,
        max_tokens: profile.max_tokens as usize,
        no_mmap: model.no_mmap,
        mlock: model.mlock,
        port: model.port,
        kv_offload: model.kv_offload,
        tensor_split: resolve_tensor_split(&model.tensor_split),
        split_mode: model.split_mode.clone(),
        vram_budget_mb: model.vram_budget_mb.clone(),
    };

    let file_hint = extract_file_hint(&request.goal);
    let file_context = file_hint
        .as_ref()
        .and_then(|p| std::fs::read_to_string(repo_root.join(p)).ok());
    let prompt = build_qwen_prompt(request, file_hint.as_deref(), file_context.as_deref());

    let mut handle = launch_server(&launch)?;

    // Load GBNF grammar if present; enforce SkyCore output shape at the sampler level.
    let grammar_path = repo_root
        .join("agents")
        .join("grammars")
        .join("skycore.gbnf");
    if grammar_path.exists() {
        match std::fs::read_to_string(&grammar_path) {
            Ok(grammar) => handle.set_grammar(Some(grammar)),
            Err(err) => {
                eprintln!(
                    "warning: failed to read grammar at {}: {err}",
                    grammar_path.display()
                );
            }
        }
    }

    let extras = SamplingExtras {
        top_k: profile.top_k,
        top_p: profile.top_p,
        min_p: profile.min_p,
        typical_p: profile.typical_p,
        repeat_last_n: profile.repeat_last_n,
        presence_penalty: profile.presence_penalty,
        frequency_penalty: profile.frequency_penalty,
        dynatemp_range: profile.dynatemp_range,
        dynatemp_exponent: profile.dynatemp_exponent,
        dry_multiplier: profile.dry_multiplier,
        dry_base: profile.dry_base,
        dry_allowed_length: profile.dry_allowed_length,
        dry_penalty_last_n: profile.dry_penalty_last_n,
        xtc_probability: profile.xtc_probability,
        xtc_threshold: profile.xtc_threshold,
    };
    handle.set_sampling(extras);

    let line = handle.call_model(&prompt)?;
    #[cfg(test)]
    append_raw_model_response(repo_root, &request.goal, &line);
    let json_text = extract_json(&line);

    if std::env::var("SKYCODE_DEBUG").is_ok() {
        eprintln!("[DEBUG] raw model response:\n{json_text}");
    }

    let raw_value = serde_json::from_str::<serde_json::Value>(json_text)?;

    let parsed = strip_provider_fields(raw_value)?;

    Ok(parsed)
}

/// Fix hunk body lines that are missing their diff prefix character.
///
/// Models often emit bare code lines inside a hunk instead of prefixing them
/// with `+`, `-`, or ` `.  For a purely additive hunk (`-0,0 +1,N`) every
/// bare line is clearly an addition, so we prefix it with `+`.  For hunks
/// that mix context and changes we leave bare lines as-is — git will reject
/// them, surfacing the model error rather than silently misapplying a patch.
fn fix_diff_prefixes(patch: &str) -> String {
    let mut out = String::with_capacity(patch.len() + 32);
    let mut in_additive_hunk = false;

    for line in patch.lines() {
        if line.starts_with("@@") {
            // Detect purely additive hunk: old range is -0,0
            in_additive_hunk = line.contains("-0,0");
            out.push_str(line);
            out.push('\n');
        } else if in_additive_hunk {
            // In a purely additive hunk (-0,0) there are no old lines, so no
            // context lines are possible.  Only `+` and `\` are valid.
            // Lines starting with ` ` look like context but are really bare
            // code lines whose `+` prefix was dropped by the model — fix them.
            if line.starts_with('+') || line.starts_with('\\') {
                out.push_str(line);
            } else {
                out.push('+');
                out.push_str(line);
            }
            out.push('\n');
        } else if line.starts_with('+')
            || line.starts_with('-')
            || line.starts_with(' ')
            || line.starts_with('\\')
        {
            // Already has a valid prefix.
            out.push_str(line);
            out.push('\n');
        } else {
            // Outside a hunk (e.g. header lines) — pass through.
            out.push_str(line);
            out.push('\n');
        }
    }

    out
}

fn extract_json(raw: &str) -> &str {
    let s = raw.trim();
    let s = s
        .strip_prefix("```json")
        .or_else(|| s.strip_prefix("```"))
        .unwrap_or(s);
    s.strip_suffix("```").unwrap_or(s).trim()
}

#[cfg(test)]
fn append_raw_model_response(repo_root: &Path, goal: &str, raw_response: &str) {
    use std::io::Write;

    let _ = (|| -> std::io::Result<()> {
        let debug_dir = repo_root.join(".skycode").join("debug");
        std::fs::create_dir_all(&debug_dir)?;
        let mut file = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(debug_dir.join("last_model_response.txt"))?;

        writeln!(file, "=== {} goal: {} ===", iso8601_utc_now(), goal)?;
        writeln!(file, "{raw_response}")?;
        writeln!(file, "----------------------------------------")?;
        Ok(())
    })();
}

#[cfg(test)]
fn iso8601_utc_now() -> String {
    let duration = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default();
    let total_seconds = duration.as_secs();
    let days = (total_seconds / 86_400) as i64;
    let seconds_of_day = total_seconds % 86_400;
    let (year, month, day) = civil_from_days(days);
    let hour = seconds_of_day / 3_600;
    let minute = (seconds_of_day % 3_600) / 60;
    let second = seconds_of_day % 60;

    format!("{year:04}-{month:02}-{day:02}T{hour:02}:{minute:02}:{second:02}Z")
}

#[cfg(test)]
fn civil_from_days(days_since_unix_epoch: i64) -> (i32, u32, u32) {
    let z = days_since_unix_epoch + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let day_of_era = z - era * 146_097;
    let year_of_era =
        (day_of_era - day_of_era / 1_460 + day_of_era / 36_524 - day_of_era / 146_096) / 365;
    let mut year = year_of_era + era * 400;
    let day_of_year = day_of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
    let month_prime = (5 * day_of_year + 2) / 153;
    let day = day_of_year - (153 * month_prime + 2) / 5 + 1;
    let month = month_prime + if month_prime < 10 { 3 } else { -9 };

    if month <= 2 {
        year += 1;
    }

    (year as i32, month as u32, day as u32)
}

fn extract_file_hint(goal: &str) -> Option<String> {
    let mut search_start = 0usize;

    while let Some(relative_start) = goal[search_start..].find("src/") {
        let abs_src_start = search_start + relative_start;

        // Walk backwards to include any crate-name prefix immediately before "src/"
        // e.g. "runtime/src/lib.rs" → include "runtime/" not just "src/lib.rs"
        let before = &goal[..abs_src_start];
        let prefix_start = before
            .rfind(|c: char| !(c.is_ascii_alphanumeric() || c == '_' || c == '-' || c == '/'))
            .map(|i| {
                i + before[i..]
                    .chars()
                    .next()
                    .map(|c| c.len_utf8())
                    .unwrap_or(1)
            })
            .unwrap_or(0);

        let end = goal[abs_src_start..]
            .char_indices()
            .find(|(_, c)| c.is_whitespace())
            .map(|(idx, _)| abs_src_start + idx)
            .unwrap_or(goal.len());

        // Validate the src/... portion has a recognised file extension
        if extract_file_match(&goal[abs_src_start..end]).is_some() {
            return Some(goal[prefix_start..end].to_string());
        }

        search_start = end;
    }

    None
}

fn extract_file_match(token: &str) -> Option<&str> {
    let mut match_end = None;

    for (dot_idx, c) in token.char_indices() {
        if c != '.' || dot_idx <= "src/".len() {
            continue;
        }

        let ext_start = dot_idx + c.len_utf8();
        let mut ext_end = ext_start;
        let mut has_extension = false;

        for (offset, ext_char) in token[ext_start..].char_indices() {
            if !(ext_char.is_ascii_alphanumeric() || ext_char == '_') {
                break;
            }

            has_extension = true;
            ext_end = ext_start + offset + ext_char.len_utf8();
        }

        if has_extension {
            match_end = Some(ext_end);
        }
    }

    match_end.map(|end| &token[..end])
}

fn build_qwen_prompt(
    request: &SkyCoreRequest,
    file_path: Option<&str>,
    file_content: Option<&str>,
) -> String {
    // When editing an existing file, ask for the complete new file content.
    // The runtime computes the diff — this is far more reliable than asking
    // the model to produce a valid unified diff.
    if let (Some(file_path), Some(file_content)) = (file_path, file_content) {
        return format!(
            "You are coder-primary. Respond ONLY with a JSON object.\n\
Task: {goal}\n\n\
The current content of {file_path} is shown below. Return the COMPLETE new file content \
in the new_content field. Do NOT omit any lines — output the full file.\n\n\
Current content:\n\
```\n{file_content}\n```\n\n\
Return exactly:\n\
{{\n\
  \"skycore_version\": \"0.1\",\n\
  \"task_id\": \"{task_id}\",\n\
  \"status\": \"ok\",\n\
  \"summary\": \"short summary\",\n\
  \"artifacts\": [\n\
    {{\n\
      \"kind\": \"diff\",\n\
      \"id\": \"rewrite-001\",\n\
      \"new_content\": \"complete new file content here\",\n\
      \"affected_files\": [\"{file_path}\"]\n\
    }}\n\
  ],\n\
  \"tool_calls_requested\": [],\n\
  \"requires_approval\": true,\n\
  \"error\": null\n\
}}",
            goal = request.goal,
            task_id = request.task_id,
            file_path = file_path,
            file_content = file_content,
        );
    }

    // New file creation: ask for complete file content so the runtime computes
    // the diff itself.
    format!(
        "You are coder-primary. Respond ONLY with a JSON object, with no markdown and no prose outside the JSON.\n\
Task: {}\n\n\
Return a SkyCore response object with this exact shape:\n\
{{\n\
  \"skycore_version\": \"0.1\",\n\
  \"task_id\": \"{}\",\n\
  \"status\": \"ok\",\n\
  \"summary\": \"short summary\",\n\
  \"artifacts\": [\n\
    {{\n\
      \"kind\": \"rewrite\",\n\
      \"id\": \"patch-001\",\n\
      \"new_content\": \"complete file content here\",\n\
      \"affected_files\": [\"relative/path.rs\"]\n\
    }}\n\
  ],\n\
  \"tool_calls_requested\": [],\n\
  \"requires_approval\": true,\n\
  \"error\": null\n\
}}\n\
Use the actual target path in affected_files, for example \"CHANGELOG.md\" for a changelog.\n\
The new_content value must be the complete file content. If you cannot produce a safe file rewrite, return an empty artifacts array.\n\
The only valid kind values are \"diff\" (for edits to existing files via unified diff) and \"rewrite\" (for full file replacement or new file creation). Do NOT invent other kinds.",
        request.goal, request.task_id
    )
}

fn store_diff_proposal(
    conn: &Connection,
    task_id: &str,
    project_id: &str,
    agent_id: &str,
    diff: &DiffProposal,
    affected_files: &Option<Vec<String>>,
) -> Result<(), OrchestratorError> {
    let now = now_unix()?;
    let expires = now + 300;
    let affected_json = serde_json::to_string(affected_files)?;

    let mut stmt = conn.prepare(
        "INSERT INTO diff_proposals (
            id, task_id, agent_id, project_id, patch_unified, base_git_ref,
            base_blob_hashes_json, affected_files_json, created_at, expires_at
        ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
    )?;

    stmt.execute(params![
        diff.id.to_string(),
        task_id,
        agent_id,
        project_id,
        diff.diff_text,
        "HEAD",
        "{}",
        affected_json,
        now,
        expires,
    ])?;

    Ok(())
}

fn now_unix() -> Result<i64, OrchestratorError> {
    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|_| OrchestratorError::InvalidSystemTime)?
        .as_secs();
    i64::try_from(secs).map_err(|_| OrchestratorError::InvalidSystemTime)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_file_hint_with_crate_prefix() {
        // The classic failure case: goal mentions "runtime/src/lib.rs"
        // Old code returned "src/lib.rs"; new code must return "runtime/src/lib.rs"
        let goal = "add a utility function is_valid_uuid(s: &str) -> bool to runtime/src/lib.rs";
        assert_eq!(
            extract_file_hint(goal),
            Some("runtime/src/lib.rs".to_string())
        );
    }

    #[test]
    fn test_extract_file_hint_bare_src() {
        // "src/lib.rs" with no prefix should still work
        assert_eq!(
            extract_file_hint("fix the bug in src/lib.rs please"),
            Some("src/lib.rs".to_string())
        );
    }

    #[test]
    fn test_extract_file_hint_none_when_no_src() {
        assert_eq!(
            extract_file_hint("explain what the orchestrator does"),
            None
        );
    }

    #[test]
    fn test_extract_file_hint_deep_path() {
        assert_eq!(
            extract_file_hint("refactor runtime/src/orchestrator/task_loop.rs"),
            Some("runtime/src/orchestrator/task_loop.rs".to_string())
        );
    }
}
