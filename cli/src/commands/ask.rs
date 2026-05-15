use std::io::{self, Write};
use std::time::{SystemTime, UNIX_EPOCH};

use clap::Args;
use rusqlite::Connection;

use skycode_orchestrator::db::migrations::run_migrations;
use skycode_orchestrator::orchestrator::{
    diff_stats, run_task_loop, OrchestratorError, TaskLoopInput,
};

use crate::commands::apply::apply_diff_from_store;
use crate::commands::approve::approve_diff;

#[derive(Debug, Args)]
pub struct AskArgs {
    /// Task goal for coder-primary.
    pub task: String,

    /// Project identifier used for memory/graph scope.
    #[arg(long, default_value = "default")]
    pub project_id: String,

    /// Repository root used for model/agent path resolution.
    #[arg(long, default_value = ".")]
    pub repo: String,

    /// Tuning profile: precise | fast | creative | deep. Default: precise.
    #[arg(long, default_value = "precise")]
    pub profile: String,

    /// Allow large destructive rewrites that remove most of an existing file.
    #[arg(long)]
    pub allow_destructive: bool,
}

pub fn run(args: &AskArgs) -> Result<(), Box<dyn std::error::Error>> {
    let db_path = std::env::current_dir()?.join("skycode.db");
    let conn = Connection::open(db_path)?;

    let migrations_dir = std::env::current_dir()?.join("memory").join("migrations");
    if migrations_dir.exists() {
        let _ = run_migrations(&conn, &migrations_dir)?;
    }

    let task_id = format!("task-{}", now_unix()?);
    let input = TaskLoopInput {
        task_id,
        project_id: args.project_id.clone(),
        goal: args.task.clone(),
        repo_root: args.repo.clone(),
        profile: args.profile.clone(),
        allow_destructive: args.allow_destructive,
    };

    let output = match run_task_loop(&conn, &input) {
        Ok(v) => v,
        Err(OrchestratorError::ModelOutputInvalid(_)) => {
            println!("Model did not return a valid diff. Try rephrasing the task.");
            return Ok(());
        }
        Err(e) => return Err(e.into()),
    };

    println!("Proposed diff:");
    println!("  id: {}", output.diff.id);
    println!("  file: {}", output.diff.file_path);
    println!("  summary: {}", output.response_summary);
    println!("---");
    println!("{}", output.diff.diff_text);
    let stats = diff_stats(&output.diff.diff_text);
    println!(
        "Diff: +{} -{} across {} file(s)",
        stats.added, stats.removed, stats.files
    );
    println!("Approve? [y/N]");

    io::stdout().flush()?;
    let mut answer = String::new();
    io::stdin().read_line(&mut answer)?;

    let approved = matches!(answer.trim().to_lowercase().as_str(), "y" | "yes");
    if !approved {
        println!("Not approved. No changes applied.");
        return Ok(());
    }

    let token = approve_diff(&output.diff.id.to_string())?;
    println!("Approved with token: {}", token.id);

    apply_diff_from_store(
        &output.diff.id.to_string(),
        std::path::Path::new(&args.repo),
    )?;
    println!("Applied successfully.");

    Ok(())
}

fn now_unix() -> Result<i64, Box<dyn std::error::Error>> {
    let secs = SystemTime::now().duration_since(UNIX_EPOCH)?.as_secs();
    Ok(i64::try_from(secs)?)
}
