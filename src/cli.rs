use std::path::PathBuf;

use clap::{Parser, Subcommand};

#[derive(Debug, Parser)]
#[command(name = "oh-my-memory")]
#[command(
    about = "A lightweight personal memory-management assistant for heavy local LLM workflows."
)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Commands,
}

#[derive(Debug, Subcommand)]
pub enum Commands {
    /// Collect one telemetry snapshot and print a summary.
    Sample {
        #[arg(long)]
        config: Option<PathBuf>,
        #[arg(long)]
        top: Option<usize>,
    },
    /// Run the low-overhead background daemon.
    Daemon {
        #[arg(long)]
        config: PathBuf,
    },
    /// Generate a compact LLM prompt or run the configured external analyzer.
    Explain {
        #[arg(long)]
        config: PathBuf,
    },
    /// Print the default config template.
    PrintConfig,
}
