use std::path::PathBuf;

use anyhow::{Context, Result};
use clap::Parser;
use tracing_subscriber::{EnvFilter, fmt};

use oh_my_memory::{
    actions::execute_plans,
    cli::{Cli, Commands},
    config::AppConfig,
    daemon,
    journal::{latest_snapshot_path, read_latest_snapshot, write_latest_snapshot},
    llm::{compact_prompt, run_external_analyzer},
    policy::evaluate,
    protect::ProtectionTracker,
    telemetry::collect_snapshot,
};

fn main() -> Result<()> {
    init_tracing();
    let cli = Cli::parse();

    match cli.command {
        Commands::Sample { config, top } => {
            let mut config = load_config_or_default(config)?;
            if let Some(top) = top {
                config.sampling.top_processes = top;
            }
            let mut snapshot = collect_snapshot(&config)?;
            let mut protection_tracker = ProtectionTracker::new();
            protection_tracker.apply(&config, &mut snapshot);
            let decision = evaluate(&config, &snapshot, 0, 0, None);
            let reports = execute_plans(&config, &snapshot, &decision);
            let latest_path = write_latest_snapshot(&config, &snapshot, &decision)?;
            println!("snapshot saved to: {}", latest_path.display());
            println!("pressure level: {}", decision.level.as_str());
            println!("memory used: {:.2}%", snapshot.used_percent());
            println!("swap used: {} MB", snapshot.used_swap_mb());
            println!("top processes:");
            for process in &snapshot.processes {
                println!(
                    "- pid={} name={} mem={}MB importance={:?}",
                    process.pid,
                    process.name,
                    process.memory_bytes / (1024 * 1024),
                    process.importance
                );
            }
            if !reports.is_empty() {
                println!("planned actions:");
                for report in reports {
                    println!("- {} => {}", report.action_id, report.detail);
                }
            }
        }
        Commands::Daemon { config } => {
            let config = AppConfig::load(&config)?;
            daemon::run(config)?;
        }
        Commands::Explain { config } => {
            let config = AppConfig::load(&config)?;
            let path = latest_snapshot_path(&config);
            if !path.exists() {
                let snapshot = collect_snapshot(&config)?;
                let decision = evaluate(&config, &snapshot, 0, 0, None);
                write_latest_snapshot(&config, &snapshot, &decision)?;
            }
            let raw = read_latest_snapshot(&path)?;
            let value: serde_json::Value = serde_json::from_str(&raw)?;
            let snapshot: oh_my_memory::models::MemorySnapshot =
                serde_json::from_value(value["snapshot"].clone())?;
            let decision: oh_my_memory::models::Decision =
                serde_json::from_value(value["decision"].clone())?;
            let prompt = compact_prompt(&snapshot, &decision);
            if let Some(output) = run_external_analyzer(&config, &prompt)? {
                println!("{}", output);
            } else {
                println!("{}", prompt);
            }
        }
        Commands::PrintConfig => {
            print!("{}", AppConfig::default_toml());
        }
    }

    Ok(())
}

fn init_tracing() {
    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"));
    fmt().with_env_filter(filter).without_time().init();
}

fn load_config_or_default(path: Option<PathBuf>) -> Result<AppConfig> {
    match path {
        Some(path) => AppConfig::load(&path),
        None => {
            let temp = AppConfig::default_toml();
            let cfg = toml::from_str(temp).context("failed to parse embedded default config")?;
            Ok(cfg)
        }
    }
}
