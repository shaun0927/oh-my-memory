use std::process::Command;

use anyhow::{Context, Result};
use tracing::warn;

use crate::{
    config::{AppConfig, OpenChromeProviderConfig, ProviderConfig},
    models::{ContextHints, PressureLevel},
};

pub trait ContextProvider {
    fn name(&self) -> &'static str;
    fn is_enabled(&self) -> bool;
    fn min_level(&self) -> PressureLevel;
    fn is_available(&self) -> bool;
    fn collect(&self) -> Result<ContextHints>;
}

pub struct TmuxProvider {
    config: ProviderConfig,
}

impl TmuxProvider {
    pub fn new(config: ProviderConfig) -> Self {
        Self { config }
    }
}

impl ContextProvider for TmuxProvider {
    fn name(&self) -> &'static str {
        "tmux"
    }

    fn is_enabled(&self) -> bool {
        self.config.enabled
    }

    fn min_level(&self) -> PressureLevel {
        self.config.min_level
    }

    fn is_available(&self) -> bool {
        Command::new("sh")
            .arg("-lc")
            .arg("command -v tmux >/dev/null 2>&1 && tmux ls >/dev/null 2>&1")
            .status()
            .map(|status| status.success())
            .unwrap_or(false)
    }

    fn collect(&self) -> Result<ContextHints> {
        let output = Command::new("sh")
            .arg("-lc")
            .arg(r#"tmux list-panes -a -F '#{pane_id}	#{pane_active}	#{pane_pid}	#{pane_current_command}'"#)
            .output()
            .context("failed to query tmux panes")?;
        if !output.status.success() {
            anyhow::bail!(
                "tmux pane query failed: {}",
                String::from_utf8_lossy(&output.stderr).trim()
            );
        }
        let stdout = String::from_utf8_lossy(&output.stdout);
        let mut hints = ContextHints {
            source: self.name().to_string(),
            ..Default::default()
        };

        for line in stdout.lines() {
            let cols = line.split('\t').collect::<Vec<_>>();
            if cols.len() != 4 {
                continue;
            }
            let pane_id = cols[0];
            let pane_active = cols[1] == "1";
            let pane_pid = cols[2].parse::<u32>().ok();
            let pane_command = cols[3];

            if pane_active {
                if let Some(pid) = pane_pid {
                    hints.protected_pids.push(pid);
                }
                hints
                    .notes
                    .push(format!("tmux active pane {pane_id} command={pane_command}"));
            }
        }

        Ok(hints)
    }
}

pub struct OpenChromeProvider {
    config: OpenChromeProviderConfig,
}

impl OpenChromeProvider {
    pub fn new(config: OpenChromeProviderConfig) -> Self {
        Self { config }
    }
}

impl ContextProvider for OpenChromeProvider {
    fn name(&self) -> &'static str {
        "openchrome"
    }

    fn is_enabled(&self) -> bool {
        self.config.enabled
    }

    fn min_level(&self) -> PressureLevel {
        self.config.min_level
    }

    fn is_available(&self) -> bool {
        !self.config.command.trim().is_empty()
    }

    fn collect(&self) -> Result<ContextHints> {
        let output = Command::new("sh")
            .arg("-lc")
            .arg(&self.config.command)
            .output()
            .with_context(|| {
                format!(
                    "failed to run openchrome provider command: {}",
                    self.config.command
                )
            })?;
        if !output.status.success() {
            anyhow::bail!(
                "openchrome provider command failed: {}",
                String::from_utf8_lossy(&output.stderr).trim()
            );
        }
        let stdout = String::from_utf8_lossy(&output.stdout);
        let hints: ContextHints =
            serde_json::from_str(&stdout).context("failed to parse openchrome context JSON")?;
        Ok(hints)
    }
}

pub fn collect_context_hints(config: &AppConfig, level: PressureLevel) -> Vec<ContextHints> {
    let providers: Vec<Box<dyn ContextProvider>> = vec![
        Box::new(TmuxProvider::new(config.context.tmux.clone())),
        Box::new(OpenChromeProvider::new(config.context.openchrome.clone())),
    ];

    let mut hints = Vec::new();
    for provider in providers {
        if !provider.is_enabled() || level < provider.min_level() || !provider.is_available() {
            continue;
        }
        match provider.collect() {
            Ok(hint) => hints.push(hint),
            Err(error) => {
                warn!(provider = provider.name(), error = %error, "context provider failed")
            }
        }
    }
    hints
}

pub fn apply_context_hints(snapshot: &mut crate::models::MemorySnapshot, hints: &[ContextHints]) {
    for hint in hints {
        for process in &mut snapshot.processes {
            if hint.protected_pids.contains(&process.pid) {
                process.runtime_protected = true;
                process
                    .protection_reasons
                    .push(format!("context_provider:{}:protected", hint.source));
            }
            if hint.stale_pids.contains(&process.pid) {
                process.external_stale_hint = true;
                process
                    .stale_reasons
                    .push(format!("context_provider:{}:stale_hint", hint.source));
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::models::{ContextHints, Importance, MemorySnapshot, ProcessFamily, ProcessSample};

    #[test]
    fn context_hints_merge_into_processes() {
        let mut snapshot = MemorySnapshot {
            timestamp_unix_secs: 0,
            total_memory_bytes: 0,
            used_memory_bytes: 0,
            available_memory_bytes: 0,
            total_swap_bytes: 0,
            used_swap_bytes: 0,
            processes: vec![ProcessSample {
                pid: 42,
                parent_pid: None,
                name: "codex".into(),
                command: "codex".into(),
                memory_bytes: 0,
                cpu_percent: 0.0,
                runtime_secs: 0,
                importance: Importance::Unknown,
                family: ProcessFamily::Agent,
                matched_profile: None,
                parent_missing: false,
                duplicate_family_count: 1,
                recent_activity: false,
                runtime_protected: false,
                protection_reasons: vec![],
                external_stale_hint: false,
                stale_score: 0,
                stale_reasons: vec![],
                cleanup_candidate: false,
                aggressive_candidate: false,
            }],
        };
        let hints = vec![ContextHints {
            source: "tmux".into(),
            protected_pids: vec![42],
            stale_pids: vec![],
            notes: vec![],
        }];
        super::apply_context_hints(&mut snapshot, &hints);
        assert!(snapshot.processes[0].runtime_protected);
    }
}
