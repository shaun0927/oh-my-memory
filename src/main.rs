use std::path::PathBuf;

use anyhow::{Context, Result};
use clap::Parser;
use tracing_subscriber::{EnvFilter, fmt};

use oh_my_memory::{
    actions::execute_plans,
    cli::{Cli, Commands, ContextCommands, IncidentCommands},
    config::AppConfig,
    context::{
        apply_context_hints, collect_context_hints, inspect_context_providers, parse_pressure_level,
    },
    daemon,
    history::apply_historical_stats,
    incident,
    journal::{latest_snapshot_path, read_latest_snapshot, write_latest_snapshot},
    llm::{compact_prompt, run_external_analyzer},
    models::Decision,
    policy::evaluate,
    protect::ProtectionTracker,
    stale::enrich_processes,
    store::Store,
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
            let base_level = oh_my_memory::policy::level_from_snapshot(&config, &snapshot);
            let context_hints = collect_context_hints(&config, base_level);
            apply_context_hints(&mut snapshot, &context_hints);
            enrich_processes(&config, &mut snapshot.processes);
            if let Some(store) = Store::open(&config)? {
                let stats = store.historical_stats(
                    &snapshot.processes,
                    config.state.history_lookback_incidents,
                )?;
                apply_historical_stats(&config, &mut snapshot.processes, &stats);
                enrich_processes(&config, &mut snapshot.processes);
            }
            let mut decision: Decision = evaluate(&config, &snapshot, 0, 0, None);
            decision.context_notes = context_hints
                .iter()
                .flat_map(|hint| hint.notes.clone())
                .collect::<Vec<_>>();
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
            if !decision.context_notes.is_empty() {
                println!("context notes:");
                for note in &decision.context_notes {
                    println!("- {}", note);
                }
            }
            if !reports.is_empty() {
                println!("planned actions:");
                for report in &reports {
                    println!("- {} => {}", report.action_id, report.detail);
                }
            }
            if let Some(store) = Store::open(&config)? {
                let incident_id = store.insert_incident(&snapshot, &decision, &reports, None)?;
                println!("incident stored: {}", incident_id);
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
        Commands::ExplainLast { config } => {
            let config = AppConfig::load(&config)?;
            if let Some(detail) = incident::latest(&config)? {
                println!("{}", compact_prompt(&detail.snapshot, &detail.decision));
            } else {
                println!("no incidents recorded");
            }
        }
        Commands::Status { config } => {
            let config = AppConfig::load(&config)?;
            if let Some(detail) = incident::latest(&config)? {
                println!("latest incident: {}", detail.summary.id);
                println!("level: {}", detail.summary.level.as_str());
                println!("used: {:.2}%", detail.summary.used_percent);
                println!("swap: {} MB", detail.summary.swap_used_mb);
                println!("actions: {}", detail.summary.action_count);
            } else {
                println!("no incidents recorded");
            }
        }
        Commands::Summary { config, limit } => {
            let config = AppConfig::load(&config)?;
            let summary = incident::summarize(&config, limit)?;
            println!("{}", serde_json::to_string_pretty(&summary)?);
        }
        Commands::Incidents { command } => match command {
            IncidentCommands::List { config, limit } => {
                let config = AppConfig::load(&config)?;
                let incidents = incident::list(&config, limit)?;
                for item in incidents {
                    println!(
                        "#{} ts={} level={} used={:.2}% swap={}MB actions={}",
                        item.id,
                        item.timestamp_unix_secs,
                        item.level.as_str(),
                        item.used_percent,
                        item.swap_used_mb,
                        item.action_count
                    );
                }
            }
            IncidentCommands::Show { config, id } => {
                let config = AppConfig::load(&config)?;
                if let Some(detail) = incident::show(&config, id)? {
                    println!("{}", serde_json::to_string_pretty(&detail)?);
                } else {
                    println!("incident not found");
                }
            }
        },
        Commands::Context { command } => match command {
            ContextCommands::Providers { config, level } => {
                let config = AppConfig::load(&config)?;
                let level = parse_pressure_level(&level)?;
                let reports = inspect_context_providers(&config, level);
                println!("{}", serde_json::to_string_pretty(&reports)?);
            }
        },
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
