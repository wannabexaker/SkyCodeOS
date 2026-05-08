use std::path::PathBuf;

use clap::Args;

use skycode_orchestrator::tools::rollback::rollback;

#[derive(Debug, Args)]
pub struct RollbackArgs {
    /// Git ref to revert to (e.g. HEAD~1 or a commit SHA).
    #[arg(default_value = "HEAD~1")]
    pub git_ref: String,

    /// Path to the repository root.
    #[arg(long, default_value = ".")]
    pub repo: PathBuf,
}

pub fn run(args: &RollbackArgs) -> Result<(), Box<dyn std::error::Error>> {
    rollback(&args.repo, &args.git_ref)?;
    println!("Rolled back to: {}", args.git_ref);
    Ok(())
}
