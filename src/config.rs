use std::{fs, path::Path};

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

use crate::models::{Importance, PressureLevel};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppConfig {
    pub sampling: SamplingConfig,
    pub thresholds: ThresholdConfig,
    pub stale: StaleConfig,
    pub protect: ProtectConfig,
    pub context: ContextConfig,
    pub llm: LlmConfig,
    pub actions: ActionConfig,
    pub profiles: Vec<ProcessProfile>,
    pub journal: JournalConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SamplingConfig {
    pub interval_secs: u64,
    pub top_processes: usize,
    pub sustained_intervals_before_llm: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ThresholdConfig {
    pub yellow_memory_percent: f64,
    pub orange_memory_percent: f64,
    pub red_memory_percent: f64,
    pub critical_memory_percent: f64,
    pub yellow_swap_mb: u64,
    pub orange_swap_mb: u64,
    pub red_swap_mb: u64,
    pub critical_swap_mb: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StaleConfig {
    pub cpu_idle_below_percent: f32,
    pub minimum_runtime_secs: u64,
    pub duplicate_family_threshold: u32,
    pub medium_memory_mb: u64,
    pub high_memory_mb: u64,
    pub cleanup_score_threshold: i32,
    pub aggressive_score_threshold: i32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProtectConfig {
    pub active_cpu_percent: f32,
    pub recent_window_secs: u64,
    pub startup_grace_secs: u64,
    pub parent_chain_depth: usize,
    pub protect_browser_main: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContextConfig {
    pub tmux: ProviderConfig,
    pub openchrome: OpenChromeProviderConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProviderConfig {
    pub enabled: bool,
    pub min_level: PressureLevel,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OpenChromeProviderConfig {
    pub enabled: bool,
    pub min_level: PressureLevel,
    pub command: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LlmConfig {
    pub enabled: bool,
    pub min_level_for_llm: PressureLevel,
    pub cooldown_secs: u64,
    pub daily_budget: u32,
    pub external_command: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ActionConfig {
    pub dry_run: bool,
    pub execute_hooks: bool,
    pub allow_destructive: bool,
    pub hooks: Vec<HookAction>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HookAction {
    pub id: String,
    pub description: String,
    pub min_level: PressureLevel,
    pub command: String,
    #[serde(default)]
    pub match_families: Vec<crate::models::ProcessFamily>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProcessProfile {
    pub name: String,
    pub importance: Importance,
    pub match_terms: Vec<String>,
    pub notes: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JournalConfig {
    pub directory: String,
    pub max_entries: usize,
}

impl AppConfig {
    pub fn load(path: &Path) -> Result<Self> {
        let raw = fs::read_to_string(path)
            .with_context(|| format!("failed to read config: {}", path.display()))?;
        let config = toml::from_str(&raw)
            .with_context(|| format!("failed to parse config: {}", path.display()))?;
        Ok(config)
    }

    pub fn default_toml() -> &'static str {
        include_str!("../config/oh-my-memory.example.toml")
    }
}
