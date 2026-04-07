use std::collections::{HashMap, HashSet};

use crate::{
    config::AppConfig,
    models::{Importance, MemorySnapshot, ProcessFamily},
};

#[derive(Debug, Default, Clone)]
pub struct ProtectionTracker {
    last_active_by_pid: HashMap<u32, u64>,
    first_seen_by_pid: HashMap<u32, u64>,
}

impl ProtectionTracker {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn apply(&mut self, config: &AppConfig, snapshot: &mut MemorySnapshot) {
        let now = snapshot.timestamp_unix_secs;
        let current_pids: HashSet<u32> = snapshot.processes.iter().map(|p| p.pid).collect();
        let parent_map: HashMap<u32, Option<u32>> = snapshot
            .processes
            .iter()
            .map(|p| (p.pid, p.parent_pid))
            .collect();

        for process in &snapshot.processes {
            self.first_seen_by_pid.entry(process.pid).or_insert(now);
            if process.cpu_percent >= config.protect.active_cpu_percent {
                self.last_active_by_pid.insert(process.pid, now);
            }
        }

        self.last_active_by_pid
            .retain(|pid, _| current_pids.contains(pid));
        self.first_seen_by_pid
            .retain(|pid, _| current_pids.contains(pid));

        let mut directly_active = HashSet::new();
        for process in &snapshot.processes {
            let first_seen = self
                .first_seen_by_pid
                .get(&process.pid)
                .copied()
                .unwrap_or(now);
            let startup_fresh = now.saturating_sub(first_seen) <= config.protect.startup_grace_secs;
            let recent_activity = self
                .last_active_by_pid
                .get(&process.pid)
                .copied()
                .is_some_and(|last| now.saturating_sub(last) <= config.protect.recent_window_secs);
            if startup_fresh
                || recent_activity
                || process.cpu_percent >= config.protect.active_cpu_percent
            {
                directly_active.insert(process.pid);
            }
        }

        let mut inherited_protection = HashSet::new();
        for pid in &directly_active {
            let mut current = Some(*pid);
            for _ in 0..config.protect.parent_chain_depth {
                let Some(curr_pid) = current else { break };
                let parent = parent_map.get(&curr_pid).copied().flatten();
                if let Some(parent_pid) = parent {
                    inherited_protection.insert(parent_pid);
                    current = Some(parent_pid);
                } else {
                    break;
                }
            }
        }

        for process in &mut snapshot.processes {
            process.recent_activity = false;
            process.runtime_protected = false;
            process.protection_reasons.clear();

            let first_seen = self
                .first_seen_by_pid
                .get(&process.pid)
                .copied()
                .unwrap_or(now);
            if now.saturating_sub(first_seen) <= config.protect.startup_grace_secs {
                process.recent_activity = true;
                process.runtime_protected = true;
                process.protection_reasons.push("startup_grace".to_string());
            }

            if self
                .last_active_by_pid
                .get(&process.pid)
                .copied()
                .is_some_and(|last| now.saturating_sub(last) <= config.protect.recent_window_secs)
            {
                process.recent_activity = true;
                process.runtime_protected = true;
                process
                    .protection_reasons
                    .push("recent_cpu_activity".to_string());
            }

            if directly_active.contains(&process.pid)
                && !process
                    .protection_reasons
                    .iter()
                    .any(|r| r == "recent_cpu_activity")
            {
                process.recent_activity = true;
                process.runtime_protected = true;
                process.protection_reasons.push("active_now".to_string());
            }

            if inherited_protection.contains(&process.pid) {
                process.runtime_protected = true;
                process
                    .protection_reasons
                    .push("parent_chain_of_active_process".to_string());
            }

            if config.protect.protect_browser_main
                && matches!(process.family, ProcessFamily::BrowserMain)
            {
                process.runtime_protected = true;
                process
                    .protection_reasons
                    .push("browser_main_family".to_string());
            }

            if matches!(process.importance, Importance::Protected) {
                process.runtime_protected = true;
                if !process
                    .protection_reasons
                    .iter()
                    .any(|r| r == "protected_profile")
                {
                    process
                        .protection_reasons
                        .push("protected_profile".to_string());
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::{
        config::AppConfig,
        models::{Importance, MemorySnapshot, ProcessFamily, ProcessSample},
    };

    fn config() -> AppConfig {
        toml::from_str(AppConfig::default_toml()).expect("default config")
    }

    #[test]
    fn recent_cpu_activity_marks_process_as_protected() {
        let cfg = config();
        let mut tracker = super::ProtectionTracker::new();
        let mut snapshot = MemorySnapshot {
            timestamp_unix_secs: 100,
            total_memory_bytes: 100,
            used_memory_bytes: 50,
            available_memory_bytes: 50,
            total_swap_bytes: 0,
            used_swap_bytes: 0,
            processes: vec![ProcessSample {
                pid: 1,
                parent_pid: None,
                name: "codex".into(),
                command: "codex".into(),
                memory_bytes: 10,
                cpu_percent: 12.0,
                runtime_secs: 120,
                importance: Importance::Recent,
                family: ProcessFamily::Agent,
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
        };

        tracker.apply(&cfg, &mut snapshot);
        assert!(snapshot.processes[0].runtime_protected);
        assert!(snapshot.processes[0].recent_activity);
    }

    #[test]
    fn parent_chain_inherits_protection() {
        let cfg = config();
        let mut tracker = super::ProtectionTracker::new();
        let mut snapshot = MemorySnapshot {
            timestamp_unix_secs: 100,
            total_memory_bytes: 100,
            used_memory_bytes: 50,
            available_memory_bytes: 50,
            total_swap_bytes: 0,
            used_swap_bytes: 0,
            processes: vec![
                ProcessSample {
                    pid: 1,
                    parent_pid: None,
                    name: "tmux".into(),
                    command: "tmux".into(),
                    memory_bytes: 10,
                    cpu_percent: 0.0,
                    runtime_secs: 1000,
                    importance: Importance::Background,
                    family: ProcessFamily::Multiplexer,
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
                },
                ProcessSample {
                    pid: 2,
                    parent_pid: Some(1),
                    name: "codex".into(),
                    command: "codex".into(),
                    memory_bytes: 10,
                    cpu_percent: 15.0,
                    runtime_secs: 300,
                    importance: Importance::Recent,
                    family: ProcessFamily::Agent,
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
                },
            ],
        };

        tracker.apply(&cfg, &mut snapshot);
        assert!(snapshot.processes[0].runtime_protected);
        assert!(snapshot.processes[1].runtime_protected);
    }
}
