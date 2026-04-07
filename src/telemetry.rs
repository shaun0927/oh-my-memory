use std::time::{SystemTime, UNIX_EPOCH};

use anyhow::Result;
use sysinfo::{ProcessRefreshKind, ProcessesToUpdate, RefreshKind, System, UpdateKind};

use crate::{
    config::AppConfig,
    fingerprint::detect_family,
    models::{Importance, MemorySnapshot, ProcessSample},
    stale::enrich_processes,
};

fn classify_process(config: &AppConfig, name: &str, command: &str) -> (Importance, Option<String>) {
    let lower_name = name.to_ascii_lowercase();
    let lower_cmd = command.to_ascii_lowercase();

    for profile in &config.profiles {
        if profile.match_terms.iter().any(|term| {
            let needle = term.to_ascii_lowercase();
            lower_name.contains(&needle) || lower_cmd.contains(&needle)
        }) {
            return (profile.importance, Some(profile.name.clone()));
        }
    }

    (Importance::Unknown, None)
}

pub fn collect_snapshot(config: &AppConfig) -> Result<MemorySnapshot> {
    let refresh = RefreshKind::nothing();
    let mut system = System::new_with_specifics(refresh);
    system.refresh_memory();
    system.refresh_processes_specifics(
        ProcessesToUpdate::All,
        true,
        ProcessRefreshKind::nothing()
            .with_memory()
            .with_cpu()
            .with_cmd(UpdateKind::OnlyIfNotSet),
    );

    let mut processes: Vec<ProcessSample> = system
        .processes()
        .iter()
        .map(|(pid, process)| {
            let name = process.name().to_string_lossy().to_string();
            let command = process
                .cmd()
                .iter()
                .map(|s| s.to_string_lossy())
                .collect::<Vec<_>>()
                .join(" ");
            let (importance, matched_profile) = classify_process(config, &name, &command);
            ProcessSample {
                pid: pid.as_u32(),
                parent_pid: process.parent().map(|pid| pid.as_u32()),
                name,
                command,
                memory_bytes: process.memory(),
                cpu_percent: process.cpu_usage(),
                runtime_secs: process.run_time(),
                importance,
                family: detect_family(
                    &process.name().to_string_lossy(),
                    &process
                        .cmd()
                        .iter()
                        .map(|s| s.to_string_lossy())
                        .collect::<Vec<_>>()
                        .join(" "),
                ),
                matched_profile,
                parent_missing: false,
                duplicate_family_count: 1,
                recent_activity: false,
                runtime_protected: false,
                protection_reasons: vec![],
                stale_score: 0,
                stale_reasons: vec![],
                cleanup_candidate: false,
                aggressive_candidate: false,
            }
        })
        .collect();

    enrich_processes(config, &mut processes);
    processes.sort_by(|a, b| b.memory_bytes.cmp(&a.memory_bytes));
    processes.truncate(config.sampling.top_processes);

    let timestamp_unix_secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();

    Ok(MemorySnapshot {
        timestamp_unix_secs,
        total_memory_bytes: system.total_memory(),
        used_memory_bytes: system.used_memory(),
        available_memory_bytes: system.available_memory(),
        total_swap_bytes: system.total_swap(),
        used_swap_bytes: system.used_swap(),
        processes,
    })
}
