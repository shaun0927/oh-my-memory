use std::{
    thread,
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use anyhow::Result;
use tracing::{info, warn};

use crate::{
    actions::execute_plans,
    config::AppConfig,
    context::{apply_context_hints, collect_context_hints},
    history::apply_historical_stats,
    journal::{append_journal_entry, write_latest_snapshot},
    llm::{compact_prompt, run_external_analyzer},
    models::PressureLevel,
    policy::evaluate,
    protect::ProtectionTracker,
    stale::enrich_processes,
    store::Store,
    telemetry::collect_snapshot,
};

pub fn run(config: AppConfig) -> Result<()> {
    let mut consecutive_high_pressure = 0usize;
    let mut daily_budget_used = 0u32;
    let mut last_llm_at: Option<u64> = None;
    let mut protection_tracker = ProtectionTracker::new();
    let store = Store::open(&config)?;

    loop {
        let mut snapshot = collect_snapshot(&config)?;
        protection_tracker.apply(&config, &mut snapshot);
        let base_level = crate::policy::level_from_snapshot(&config, &snapshot);
        let context_hints = collect_context_hints(&config, base_level);
        apply_context_hints(&mut snapshot, &context_hints);
        enrich_processes(&config, &mut snapshot.processes);
        if let Some(store) = &store {
            let stats = store
                .historical_stats(&snapshot.processes, config.state.history_lookback_incidents)?;
            apply_historical_stats(&config, &mut snapshot.processes, &stats);
            enrich_processes(&config, &mut snapshot.processes);
        }
        let seconds_since_last_llm =
            last_llm_at.map(|last| snapshot.timestamp_unix_secs.saturating_sub(last));
        let mut decision = evaluate(
            &config,
            &snapshot,
            consecutive_high_pressure,
            daily_budget_used,
            seconds_since_last_llm,
        );
        decision.context_notes = context_hints
            .iter()
            .flat_map(|hint| hint.notes.clone())
            .collect::<Vec<_>>();

        if decision.level >= PressureLevel::Orange {
            consecutive_high_pressure += 1;
        } else {
            consecutive_high_pressure = 0;
        }

        let llm_output = if decision.llm_recommended {
            let prompt = compact_prompt(&snapshot, &decision);
            match run_external_analyzer(&config, &prompt) {
                Ok(output) => {
                    daily_budget_used += 1;
                    last_llm_at = Some(snapshot.timestamp_unix_secs);
                    output
                }
                Err(error) => {
                    warn!(error = %error, "external llm analyzer failed");
                    None
                }
            }
        } else {
            None
        };

        let reports = execute_plans(&config, &snapshot, &decision);
        write_latest_snapshot(&config, &snapshot, &decision)?;
        append_journal_entry(
            &config,
            &snapshot,
            &decision,
            &reports,
            llm_output.as_deref(),
        )?;
        if let Some(store) = &store {
            store.insert_incident(&snapshot, &decision, &reports, llm_output.as_deref())?;
        }

        info!(
            level = %decision.level.as_str(),
            used_percent = snapshot.used_percent(),
            swap_mb = snapshot.used_swap_mb(),
            actions = reports.len(),
            "decision recorded"
        );

        thread::sleep(Duration::from_secs(config.sampling.interval_secs));

        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        if now % 86_400 < config.sampling.interval_secs {
            daily_budget_used = 0;
        }
    }
}
