use std::path::PathBuf;

use clap::Args;

use skycode_orchestrator::tools::diff::create_diff;
use skycode_orchestrator::tools::filesystem::read_file;

#[derive(Debug, Args)]
pub struct DiffArgs {
    /// Path to the file to generate a diff for.
    pub file: PathBuf,
}

pub fn run(args: &DiffArgs) -> Result<(), Box<dyn std::error::Error>> {
    let content = read_file(&args.file)?;
    // In V1 CLI, we generate a stub diff showing the current content as "before"
    // with an empty "after". The real agent loop will supply the actual edit.
    let proposal = create_diff(&args.file, &content, "")?;

    println!("Diff proposal created:");
    println!("  id:         {}", proposal.id);
    println!("  file:       {}", proposal.file_path);
    println!("  created_at: {}", proposal.created_at);
    println!("---");
    println!("{}", proposal.diff_text);

    Ok(())
}
