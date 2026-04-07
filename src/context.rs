use std::process::Command;

use anyhow::{Context, Result, anyhow};
use serde::{Deserialize, Serialize};
use tracing::warn;

use crate::{
    config::{AppConfig, ExternalProviderConfig, OpenChromeProviderConfig, ProviderConfig},
    models::{ContextHints, PressureLevel},
};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OpenChromeContextPayload {
    pub schema_version: u32,
    pub source: String,
    #[serde(default)]
    pub protected_pids: Vec<u32>,
    #[serde(default)]
    pub stale_pids: Vec<u32>,
    #[serde(default)]
    pub notes: Vec<String>,
    #[serde(default)]
    pub active_workers: Vec<String>,
    #[serde(default)]
    pub stale_workers: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentContextPayload {
    pub schema_version: u32,
    pub source: String,
    #[serde(default)]
    pub protected_pids: Vec<u32>,
    #[serde(default)]
    pub stale_pids: Vec<u32>,
    #[serde(default)]
    pub recent_pids: Vec<u32>,
    #[serde(default)]
    pub notes: Vec<String>,
    #[serde(default)]
    pub active_sessions: Vec<String>,
    #[serde(default)]
    pub idle_sessions: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ProviderInspection {
    pub name: String,
    pub enabled: bool,
    pub available: bool,
    pub min_level: String,
    pub collected: Option<ContextHints>,
    pub skipped_reason: Option<String>,
}

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

pub struct AgentMetadataProvider {
    config: ExternalProviderConfig,
}

impl AgentMetadataProvider {
    pub fn new(config: ExternalProviderConfig) -> Self {
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
        let payload: OpenChromeContextPayload =
            serde_json::from_str(&stdout).context("failed to parse openchrome context JSON")?;
        if payload.schema_version != 1 {
            return Err(anyhow!(
                "unsupported openchrome schema_version={}",
                payload.schema_version
            ));
        }
        let mut notes = payload.notes;
        if !payload.active_workers.is_empty() {
            notes.push(format!(
                "openchrome active_workers={}",
                payload.active_workers.join(",")
            ));
        }
        if !payload.stale_workers.is_empty() {
            notes.push(format!(
                "openchrome stale_workers={}",
                payload.stale_workers.join(",")
            ));
        }
        Ok(ContextHints {
            source: payload.source,
            protected_pids: payload.protected_pids,
            stale_pids: payload.stale_pids,
            recent_pids: vec![],
            notes,
        })
    }
}

impl ContextProvider for AgentMetadataProvider {
    fn name(&self) -> &'static str {
        "agents"
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
                    "failed to run agent provider command: {}",
                    self.config.command
                )
            })?;
        if !output.status.success() {
            anyhow::bail!(
                "agent provider command failed: {}",
                String::from_utf8_lossy(&output.stderr).trim()
            );
        }
        let stdout = String::from_utf8_lossy(&output.stdout);
        let payload: AgentContextPayload =
            serde_json::from_str(&stdout).context("failed to parse agent context JSON")?;
        if payload.schema_version != 1 {
            return Err(anyhow!(
                "unsupported agent schema_version={}",
                payload.schema_version
            ));
        }
        let mut notes = payload.notes;
        if !payload.active_sessions.is_empty() {
            notes.push(format!(
                "agents active_sessions={}",
                payload.active_sessions.join(",")
            ));
        }
        if !payload.idle_sessions.is_empty() {
            notes.push(format!(
                "agents idle_sessions={}",
                payload.idle_sessions.join(",")
            ));
        }
        Ok(ContextHints {
            source: payload.source,
            protected_pids: payload.protected_pids,
            stale_pids: payload.stale_pids,
            recent_pids: payload.recent_pids,
            notes,
        })
    }
}

pub fn collect_context_hints(config: &AppConfig, level: PressureLevel) -> Vec<ContextHints> {
    let providers: Vec<Box<dyn ContextProvider>> = vec![
        Box::new(TmuxProvider::new(config.context.tmux.clone())),
        Box::new(OpenChromeProvider::new(config.context.openchrome.clone())),
        Box::new(AgentMetadataProvider::new(config.context.agents.clone())),
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

pub fn inspect_context_providers(
    config: &AppConfig,
    level: PressureLevel,
) -> Vec<ProviderInspection> {
    let providers: Vec<Box<dyn ContextProvider>> = vec![
        Box::new(TmuxProvider::new(config.context.tmux.clone())),
        Box::new(OpenChromeProvider::new(config.context.openchrome.clone())),
        Box::new(AgentMetadataProvider::new(config.context.agents.clone())),
    ];

    providers
        .into_iter()
        .map(|provider| {
            let enabled = provider.is_enabled();
            let available = provider.is_available();
            let min_level = provider.min_level();

            if !enabled {
                return ProviderInspection {
                    name: provider.name().to_string(),
                    enabled,
                    available,
                    min_level: min_level.as_str().to_string(),
                    collected: None,
                    skipped_reason: Some("disabled".to_string()),
                };
            }

            if level < min_level {
                return ProviderInspection {
                    name: provider.name().to_string(),
                    enabled,
                    available,
                    min_level: min_level.as_str().to_string(),
                    collected: None,
                    skipped_reason: Some(format!(
                        "level {} below provider minimum {}",
                        level.as_str(),
                        min_level.as_str()
                    )),
                };
            }

            if !available {
                return ProviderInspection {
                    name: provider.name().to_string(),
                    enabled,
                    available,
                    min_level: min_level.as_str().to_string(),
                    collected: None,
                    skipped_reason: Some("not available".to_string()),
                };
            }

            match provider.collect() {
                Ok(hints) => ProviderInspection {
                    name: provider.name().to_string(),
                    enabled,
                    available,
                    min_level: min_level.as_str().to_string(),
                    collected: Some(hints),
                    skipped_reason: None,
                },
                Err(error) => ProviderInspection {
                    name: provider.name().to_string(),
                    enabled,
                    available,
                    min_level: min_level.as_str().to_string(),
                    collected: None,
                    skipped_reason: Some(error.to_string()),
                },
            }
        })
        .collect()
}

pub fn parse_pressure_level(raw: &str) -> Result<PressureLevel> {
    match raw.to_ascii_lowercase().as_str() {
        "green" => Ok(PressureLevel::Green),
        "yellow" => Ok(PressureLevel::Yellow),
        "orange" => Ok(PressureLevel::Orange),
        "red" => Ok(PressureLevel::Red),
        "critical" => Ok(PressureLevel::Critical),
        _ => Err(anyhow!("invalid pressure level: {raw}")),
    }
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
            if hint.recent_pids.contains(&process.pid) {
                process.recent_activity = true;
                process.runtime_protected = true;
                process
                    .protection_reasons
                    .push(format!("context_provider:{}:recent", hint.source));
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
                historical_sightings: 0,
                historical_stale_hits: 0,
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
            recent_pids: vec![],
            notes: vec![],
        }];
        super::apply_context_hints(&mut snapshot, &hints);
        assert!(snapshot.processes[0].runtime_protected);
    }

    #[test]
    fn openchrome_payload_parses_with_schema_v1() {
        let raw = r#"{
          "schema_version": 1,
          "source": "openchrome",
          "protected_pids": [111, 222],
          "stale_pids": [333],
          "notes": ["active browser session attached"],
          "active_workers": ["default"],
          "stale_workers": ["stale-1"]
        }"#;
        let payload: super::OpenChromeContextPayload = serde_json::from_str(raw).expect("payload");
        assert_eq!(payload.schema_version, 1);
        assert_eq!(payload.protected_pids.len(), 2);
        assert_eq!(payload.stale_pids, vec![333]);
    }

    #[test]
    fn agent_payload_parses_with_schema_v1() {
        let raw = r#"{
          "schema_version": 1,
          "source": "agents",
          "protected_pids": [444],
          "stale_pids": [555],
          "recent_pids": [666],
          "notes": ["codex session is currently active"],
          "active_sessions": ["codex-main"],
          "idle_sessions": ["claude-idle-1"]
        }"#;
        let payload: super::AgentContextPayload = serde_json::from_str(raw).expect("payload");
        assert_eq!(payload.source, "agents");
        assert_eq!(payload.recent_pids, vec![666]);
        assert_eq!(payload.idle_sessions.len(), 1);
    }
}
