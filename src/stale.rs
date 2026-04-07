use std::collections::{HashMap, HashSet};

use crate::{
    config::{AppConfig, ProcessProfile},
    models::{Importance, ProcessFamily, ProcessSample},
};

pub fn enrich_processes(config: &AppConfig, processes: &mut [ProcessSample]) {
    let all_pids: HashSet<u32> = processes.iter().map(|p| p.pid).collect();
    let mut family_counts: HashMap<(ProcessFamily, String), u32> = HashMap::new();

    for process in processes.iter() {
        let key = (process.family, process.name.to_ascii_lowercase());
        *family_counts.entry(key).or_insert(0) += 1;
    }

    for process in processes.iter_mut() {
        process.parent_missing = process
            .parent_pid
            .is_some_and(|pid| !all_pids.contains(&pid));
        let key = (process.family, process.name.to_ascii_lowercase());
        process.duplicate_family_count = family_counts.get(&key).copied().unwrap_or(1);

        let (score, reasons) = stale_score(config, process);
        process.stale_score = score;
        process.stale_reasons = reasons;
        process.cleanup_candidate = score >= config.stale.cleanup_score_threshold;
        process.aggressive_candidate = score >= config.stale.aggressive_score_threshold;
    }
}

fn stale_score(config: &AppConfig, process: &ProcessSample) -> (i32, Vec<String>) {
    let mut score = 0;
    let mut reasons = Vec::new();
    let memory_mb = process.memory_bytes / (1024 * 1024);

    if memory_mb >= config.stale.high_memory_mb {
        score += 25;
        reasons.push(format!("high_memory_mb={memory_mb}"));
    } else if memory_mb >= config.stale.medium_memory_mb {
        score += 15;
        reasons.push(format!("medium_memory_mb={memory_mb}"));
    }

    if process.cpu_percent <= config.stale.cpu_idle_below_percent {
        score += 20;
        reasons.push(format!("low_cpu={:.2}", process.cpu_percent));
    }

    if process.runtime_secs >= config.stale.minimum_runtime_secs {
        score += 15;
        reasons.push(format!("long_runtime_secs={}", process.runtime_secs));
    }

    if process.parent_missing {
        score += 25;
        reasons.push("parent_missing".to_string());
    }

    if process.duplicate_family_count >= config.stale.duplicate_family_threshold {
        score += 10;
        reasons.push(format!(
            "duplicate_family_count={}",
            process.duplicate_family_count
        ));
    }

    match process.importance {
        Importance::Protected => {
            score -= 50;
            reasons.push("protected".to_string());
        }
        Importance::Recent => {
            score -= 30;
            reasons.push("recent".to_string());
        }
        Importance::Background | Importance::Unknown => {}
    }

    (score, reasons)
}

pub fn is_hook_applicable(hook_profiles: &[ProcessFamily], candidate: &ProcessSample) -> bool {
    hook_profiles.is_empty() || hook_profiles.contains(&candidate.family)
}

pub fn best_profile_match<'a>(
    profiles: &'a [ProcessProfile],
    candidate: &ProcessSample,
) -> Option<&'a ProcessProfile> {
    profiles.iter().find(|profile| {
        profile
            .match_terms
            .iter()
            .any(|term| candidate.name.contains(term) || candidate.command.contains(term))
    })
}

#[cfg(test)]
mod tests {
    use crate::{
        config::AppConfig,
        models::{Importance, ProcessFamily, ProcessSample},
    };

    fn config() -> AppConfig {
        toml::from_str(AppConfig::default_toml()).expect("default config")
    }

    #[test]
    fn protected_processes_are_penalized() {
        let cfg = config();
        let process = ProcessSample {
            pid: 1,
            parent_pid: None,
            name: "Google Chrome".into(),
            command: "Google Chrome".into(),
            memory_bytes: 500 * 1024 * 1024,
            cpu_percent: 0.1,
            runtime_secs: 1000,
            importance: Importance::Protected,
            family: ProcessFamily::BrowserMain,
            matched_profile: None,
            parent_missing: false,
            duplicate_family_count: 1,
            stale_score: 0,
            stale_reasons: vec![],
            cleanup_candidate: false,
            aggressive_candidate: false,
        };
        let (score, _) = super::stale_score(&cfg, &process);
        assert!(score < cfg.stale.cleanup_score_threshold);
    }
}
