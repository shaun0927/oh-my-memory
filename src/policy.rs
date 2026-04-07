use crate::{
    config::AppConfig,
    models::{ActionPlan, Decision, Importance, MemorySnapshot, PressureLevel},
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

    let background_count = snapshot
        .processes
        .iter()
        .filter(|p| matches!(p.importance, Importance::Background | Importance::Unknown))
        .count();

    if background_count > 0 {
        for hook in &config.actions.hooks {
            if level >= hook.min_level {
                plans.push(ActionPlan {
                    id: hook.id.clone(),
                    description: hook.description.clone(),
                    min_level: hook.min_level,
                    command: Some(hook.command.clone()),
                    safe_by_default: true,
                });
            }
        }
    }

    if plans.is_empty() && level >= PressureLevel::Yellow {
        plans.push(ActionPlan {
            id: "observe_only".to_string(),
            description: "Stay in observe-only mode and ask the user to inspect protected foreground workloads before stronger remediation.".to_string(),
            min_level: PressureLevel::Yellow,
            command: None,
            safe_by_default: true,
        });
    }

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
        models::{Importance, MemorySnapshot, PressureLevel, ProcessSample},
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
                name: "chrome".into(),
                command: "chrome".into(),
                memory_bytes: 10,
                cpu_percent: 0.0,
                importance: Importance::Background,
                matched_profile: None,
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
