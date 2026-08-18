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
#[command(about = "Galfus Script tooling")]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
    Run {
        #[arg(default_value = ".")]
        workspace: String,
        #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
        args: Vec<String>,
    },
    Init,
    Check {
        workspace: String,
    },
    Compile {
        workspace: String,
        #[arg(short, long)]
        target: Option<String>,
        #[arg(short, long)]
        out: Option<String>,
        #[arg(short, long, default_value = "fastest")]
        profile: String,
    },
    Upgrade {
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
