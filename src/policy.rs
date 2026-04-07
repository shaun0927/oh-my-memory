use crate::{
    config::AppConfig,
    models::{ActionKind, ActionPlan, Decision, MemorySnapshot, PressureLevel, ProcessFamily},
    stale::is_hook_applicable,
};

pub fn level_from_snapshot(config: &AppConfig, snapshot: &MemorySnapshot) -> PressureLevel {
    let used = snapshot.used_percent();
    let swap_mb = snapshot.used_swap_mb();
    let t = &config.thresholds;

    if used >= t.critical_memory_percent || swap_mb >= t.critical_swap_mb {
        PressureLevel::Critical
    } else if used >= t.red_memory_percent || swap_mb >= t.red_swap_mb {
        PressureLevel::Red
    } else if used >= t.orange_memory_percent || swap_mb >= t.orange_swap_mb {
        PressureLevel::Orange
    } else if used >= t.yellow_memory_percent || swap_mb >= t.yellow_swap_mb {
        PressureLevel::Yellow
    } else {
        PressureLevel::Green
    }
}

pub fn should_invoke_llm(
    config: &AppConfig,
    level: PressureLevel,
    consecutive_high_pressure: usize,
    daily_budget_used: u32,
    seconds_since_last_llm: Option<u64>,
) -> bool {
    if !config.llm.enabled {
        return false;
    }
    if level < config.llm.min_level_for_llm {
        return false;
    }
    if consecutive_high_pressure < config.sampling.sustained_intervals_before_llm {
        return false;
    }
    if daily_budget_used >= config.llm.daily_budget {
        return false;
    }
    if let Some(elapsed) = seconds_since_last_llm {
        if elapsed < config.llm.cooldown_secs {
            return false;
        }
    }
    true
}

pub fn plan_actions(
    config: &AppConfig,
    level: PressureLevel,
    snapshot: &MemorySnapshot,
) -> Vec<ActionPlan> {
    let mut plans = Vec::new();
    let mut candidates = snapshot
        .processes
        .iter()
        .filter(|p| p.cleanup_candidate)
        .collect::<Vec<_>>();
    candidates.sort_by(|a, b| b.stale_score.cmp(&a.stale_score));

    for hook in &config.actions.hooks {
        if level < hook.min_level {
            continue;
        }
        let matched = candidates
            .iter()
            .filter(|candidate| {
                !candidate.runtime_protected && is_hook_applicable(&hook.match_families, candidate)
            })
            .map(|candidate| candidate.pid)
            .collect::<Vec<_>>();
        if !matched.is_empty() {
            plans.push(ActionPlan {
                id: hook.id.clone(),
                kind: ActionKind::Hook,
                description: hook.description.clone(),
                min_level: hook.min_level,
                command: Some(hook.command.clone()),
                safe_by_default: true,
                priority: 10,
                target_pids: matched,
                rationale: candidates
                    .iter()
                    .filter(|candidate| {
                        !candidate.runtime_protected
                            && is_hook_applicable(&hook.match_families, candidate)
                    })
                    .flat_map(|candidate| candidate.stale_reasons.clone())
                    .take(5)
                    .collect(),
            });
        }
    }

    if level >= PressureLevel::Red {
        let generic_targets = candidates
            .iter()
            .filter(|candidate| {
                !candidate.runtime_protected
                    && matches!(
                        candidate.family,
                        ProcessFamily::Watcher
                            | ProcessFamily::BuildTool
                            | ProcessFamily::Helper
                            | ProcessFamily::Unknown
                    )
            })
            .take(3)
            .map(|candidate| candidate.pid)
            .collect::<Vec<_>>();
        if !generic_targets.is_empty() {
            plans.push(ActionPlan {
                id: "graceful_terminate_candidates".to_string(),
                kind: ActionKind::GracefulTerminate,
                description: "Gracefully terminate the safest stale candidates.".to_string(),
                min_level: PressureLevel::Red,
                command: None,
                safe_by_default: true,
                priority: 20,
                target_pids: generic_targets,
                rationale: vec!["stale candidates exceeded cleanup threshold".to_string()],
            });
        }
    }

    if level >= PressureLevel::Critical && config.actions.allow_destructive {
        let aggressive_targets = candidates
            .iter()
            .filter(|candidate| candidate.aggressive_candidate)
            .filter(|candidate| !candidate.runtime_protected)
            .take(2)
            .map(|candidate| candidate.pid)
            .collect::<Vec<_>>();
        if !aggressive_targets.is_empty() {
            plans.push(ActionPlan {
                id: "hard_terminate_aggressive_candidates".to_string(),
                kind: ActionKind::HardTerminate,
                description: "Hard terminate only the highest-confidence stale candidates."
                    .to_string(),
                min_level: PressureLevel::Critical,
                command: None,
                safe_by_default: false,
                priority: 100,
                target_pids: aggressive_targets,
                rationale: vec!["critical pressure with aggressive stale candidates".to_string()],
            });
        }
    }

    if plans.is_empty() && level >= PressureLevel::Yellow {
        plans.push(ActionPlan {
            id: "observe_only".to_string(),
            kind: ActionKind::Observe,
            description: "Stay in observe-only mode and ask the user to inspect protected foreground workloads before stronger remediation.".to_string(),
            min_level: PressureLevel::Yellow,
            command: None,
            safe_by_default: true,
            priority: 0,
            target_pids: vec![],
            rationale: vec!["no safe stale candidates were found".to_string()],
        });
    }

    plans.sort_by_key(|plan| plan.priority);
    plans
}

pub fn evaluate(
    config: &AppConfig,
    snapshot: &MemorySnapshot,
    consecutive_high_pressure: usize,
    daily_budget_used: u32,
    seconds_since_last_llm: Option<u64>,
) -> Decision {
    let level = level_from_snapshot(config, snapshot);
    let llm_recommended = should_invoke_llm(
        config,
        level,
        consecutive_high_pressure,
        daily_budget_used,
        seconds_since_last_llm,
    );

    let reasons = vec![
        format!("memory_used_percent={:.2}", snapshot.used_percent()),
        format!("swap_used_mb={}", snapshot.used_swap_mb()),
        format!("top_processes={}", snapshot.processes.len()),
        format!("pressure_level={}", level.as_str()),
        format!(
            "cleanup_candidates={}",
            snapshot
                .processes
                .iter()
                .filter(|p| p.cleanup_candidate)
                .count()
        ),
    ];

    let planned_actions = plan_actions(config, level, snapshot);

    Decision {
        level,
        reasons,
        llm_recommended,
        planned_actions,
    }
}

#[cfg(test)]
mod tests {
    use crate::{
        config::AppConfig,
        models::{Importance, MemorySnapshot, PressureLevel, ProcessFamily, ProcessSample},
    };

    fn test_config() -> AppConfig {
        toml::from_str(AppConfig::default_toml()).expect("default config")
    }

    fn snapshot(used_percent: f64, swap_mb: u64) -> MemorySnapshot {
        let total = 1000_u64;
        let used = (used_percent / 100.0 * total as f64) as u64;
        MemorySnapshot {
            timestamp_unix_secs: 0,
            total_memory_bytes: total,
            used_memory_bytes: used,
            available_memory_bytes: total.saturating_sub(used),
            total_swap_bytes: 10_000 * 1024 * 1024,
            used_swap_bytes: swap_mb * 1024 * 1024,
            processes: vec![ProcessSample {
                pid: 1,
                parent_pid: None,
                name: "chrome".into(),
                command: "chrome".into(),
                memory_bytes: 10,
                cpu_percent: 0.0,
                runtime_secs: 0,
                importance: Importance::Background,
                family: ProcessFamily::BrowserMain,
                matched_profile: None,
                parent_missing: false,
                duplicate_family_count: 1,
                recent_activity: false,
                runtime_protected: false,
                protection_reasons: vec![],
                stale_score: 0,
                stale_reasons: vec![],
                cleanup_candidate: false,
                aggressive_candidate: false,
            }],
        }
    }

    #[test]
    fn levels_escalate_by_threshold() {
        let config = test_config();
        assert_eq!(
            super::level_from_snapshot(&config, &snapshot(20.0, 0)),
            PressureLevel::Green
        );
        assert_eq!(
            super::level_from_snapshot(&config, &snapshot(76.0, 0)),
            PressureLevel::Yellow
        );
        assert_eq!(
            super::level_from_snapshot(&config, &snapshot(86.0, 0)),
            PressureLevel::Orange
        );
        assert_eq!(
            super::level_from_snapshot(&config, &snapshot(93.0, 0)),
            PressureLevel::Red
        );
    }

    #[test]
    fn llm_gate_respects_budget_and_cooldown() {
        let mut config = test_config();
        config.llm.enabled = true;
        let yes = super::should_invoke_llm(&config, PressureLevel::Orange, 4, 0, Some(999));
        assert!(yes);
        let no_budget = super::should_invoke_llm(&config, PressureLevel::Orange, 4, 99, Some(999));
        assert!(!no_budget);
        let no_cooldown = super::should_invoke_llm(&config, PressureLevel::Orange, 4, 0, Some(10));
        assert!(!no_cooldown);
    }
}
