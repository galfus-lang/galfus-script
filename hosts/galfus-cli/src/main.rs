mod compile;
mod diagnostics;
mod init;
mod upgrade;
mod workspace;

use std::process;

use anyhow::Result;
use clap::{Parser, Subcommand, ValueEnum};

#[derive(Debug, Clone, ValueEnum)]
pub enum ReleaseTag {
    Alpha,
    Beta,
    Stable,
    Next,
    Latest,
}

#[derive(Debug, Parser)]
#[command(name = "galfus")]
#[command(version)]
#[command(about = "Galfus Script Toolchain and Runtime")]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
    /// Run the current project
    Run {
        /// Path to the project workspace directory (defaults to current directory)
        #[arg(default_value = ".")]
        workspace: String,
        /// Additional arguments passed to the script or application
        #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
        args: Vec<String>,
    },
    /// Initialize a new Galfus project in the current directory
    Init,
    /// Check the project for errors without running or compiling it
    Check {
        /// Path to the project workspace directory
        workspace: String,
    },
    /// Compile the project into an executable or binary format
    Compile {
        /// Path to the project workspace directory
        workspace: String,
        /// Target architecture or platform (e.g., x86_64-linux)
        #[arg(short, long)]
        target: Option<String>,
        /// Output path for the compiled artifact
        #[arg(short, long)]
        out: Option<String>,
        /// Optimization profile to use during compilation
        #[arg(short, long, default_value = "fastest")]
        profile: String,
    },
    /// Upgrade the Galfus CLI binary to a newer version
    Upgrade {
        /// Release channel or specific tag to download
        #[arg(short, long, default_value = "latest")]
        tag: ReleaseTag,
    },
}

fn main() -> Result<()> {
    tracing_subscriber::fmt::init();
    match Cli::parse().command {
        Command::Run { workspace, args } => {
            let exit_code = workspace::run_project(&workspace, &args)?;
            process::exit(exit_code);
        }
        Command::Init => init::run_init(),
        Command::Check { workspace } => workspace::check_workspace_root(&workspace),
        Command::Compile {
            workspace,
            target,
            out,
            profile,
        } => compile::run_compile(&workspace, target, out, &profile),
        Command::Upgrade { tag } => upgrade::run_upgrade(tag),
    }
}
