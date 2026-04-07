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
    /// Explain the latest recorded incident from persistent state.
    ExplainLast {
        #[arg(long)]
        config: PathBuf,
    },
    /// Show a compact status summary from the latest recorded incident.
    Status {
        #[arg(long)]
        config: PathBuf,
    },
    /// Inspect incident history.
    Incidents {
        #[command(subcommand)]
        command: IncidentCommands,
    },
    /// Inspect optional context providers and the hints they return.
    Context {
        #[command(subcommand)]
        command: ContextCommands,
    },
    /// Print the default config template.
    PrintConfig,
}

#[derive(Debug, Subcommand)]
pub enum IncidentCommands {
    /// List recent incidents.
    List {
        #[arg(long)]
        config: PathBuf,
        #[arg(long, default_value_t = 10)]
        limit: usize,
    },
    /// Show one incident in detail.
    Show {
        #[arg(long)]
        config: PathBuf,
        id: i64,
    },
}

#[derive(Debug, Subcommand)]
pub enum ContextCommands {
    /// List provider availability and current gating state.
    Providers {
        #[arg(long)]
        config: PathBuf,
        #[arg(long, default_value = "green")]
        level: String,
    },
}
