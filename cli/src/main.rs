mod commands;

use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(
    name = "skycode",
    version,
    about = "SkyCodeOS — local offline coding assistant"
)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Run persistent coder task loop with approval prompt.
    Ask(commands::ask::AskArgs),
    /// Generate a diff proposal for a file.
    Diff(commands::diff::DiffArgs),
    /// Approve a diff proposal by its ID.
    Approve(commands::approve::ApproveArgs),
    /// Apply an approved diff proposal by its ID.
    Apply(commands::apply::ApplyArgs),
    /// Rollback to a previous git ref.
    Rollback(commands::rollback::RollbackArgs),
    /// Scan a project directory and index it.
    Scan(commands::scan::ScanArgs),
    /// Query graph dependencies.
    Graph {
        #[command(subcommand)]
        command: GraphCommands,
    },
    /// Model lifecycle and benchmarking commands.
    Model {
        #[command(subcommand)]
        command: commands::model::ModelCommands,
    },
    /// Tuning profile commands.
    Profile {
        #[command(subcommand)]
        command: commands::profile::ProfileCommands,
    },
}

#[derive(Subcommand)]
enum GraphCommands {
    /// Show impact of changes to a symbol/node.
    Impact(commands::scan::GraphImpactArgs),
}

fn main() {
    let cli = Cli::parse();

    let result = match &cli.command {
        Commands::Ask(args) => commands::ask::run(args),
        Commands::Diff(args) => commands::diff::run(args),
        Commands::Approve(args) => commands::approve::run(args),
        Commands::Apply(args) => commands::apply::run(args),
        Commands::Rollback(args) => commands::rollback::run(args),
        Commands::Scan(args) => commands::scan::run_scan(args),
        Commands::Graph { command } => match command {
            GraphCommands::Impact(args) => commands::scan::run_graph_impact(args),
        },
        Commands::Model { command } => commands::model::run_model_command(command),
        Commands::Profile { command } => commands::profile::run_profile_command(command),
    };

    if let Err(err) = result {
        eprintln!("error: {err}");
        std::process::exit(1);
    }
}
