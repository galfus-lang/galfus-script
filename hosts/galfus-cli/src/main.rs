mod native_io;
mod workspace;

use std::process;

use anyhow::Result;
use clap::{Parser, Subcommand};

#[derive(Debug, Parser)]
#[command(name = "galfus")]
#[command(about = "Galfus Script tooling")]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
    Run {
        workspace: String,
        #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
        args: Vec<String>,
    },
    Check {
        workspace: String,
    },
    Compile {
        workspace: String,
        #[arg(short, long)]
        target: String,
        #[arg(short, long)]
        out: String,
    },
}

fn main() -> Result<()> {
    tracing_subscriber::fmt::init();
    match Cli::parse().command {
        Command::Run { workspace, args } => {
            let exit_code = workspace::run_project(&workspace, &args)?;
            process::exit(exit_code);
        }
        Command::Check { workspace } => workspace::check_workspace_root(&workspace),
        Command::Compile {
            workspace,
            target,
            out,
        } => workspace::compile_workspace(&workspace, &target, &out),
    }
}
