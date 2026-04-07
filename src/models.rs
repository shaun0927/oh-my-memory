use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "lowercase")]
pub enum PressureLevel {
    Green,
    Yellow,
    Orange,
    Red,
    Critical,
}

impl PressureLevel {
    pub fn as_str(self) -> &'static str {
        match self {
            PressureLevel::Green => "green",
            PressureLevel::Yellow => "yellow",
            PressureLevel::Orange => "orange",
            PressureLevel::Red => "red",
            PressureLevel::Critical => "critical",
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Importance {
    Protected,
    Recent,
    Background,
    Unknown,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum ProcessFamily {
    BrowserMain,
    BrowserAutomation,
    Agent,
    Multiplexer,
    BuildTool,
    Watcher,
    Helper,
    Unknown,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProcessSample {
    pub pid: u32,
    pub parent_pid: Option<u32>,
    pub name: String,
    pub command: String,
    pub memory_bytes: u64,
    pub cpu_percent: f32,
    pub runtime_secs: u64,
    pub importance: Importance,
    pub family: ProcessFamily,
    pub matched_profile: Option<String>,
    pub parent_missing: bool,
    pub duplicate_family_count: u32,
    pub recent_activity: bool,
    pub runtime_protected: bool,
    pub protection_reasons: Vec<String>,
    pub external_stale_hint: bool,
    pub stale_score: i32,
    pub stale_reasons: Vec<String>,
    pub cleanup_candidate: bool,
    pub aggressive_candidate: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemorySnapshot {
    pub timestamp_unix_secs: u64,
    pub total_memory_bytes: u64,
    pub used_memory_bytes: u64,
    pub available_memory_bytes: u64,
    pub total_swap_bytes: u64,
    pub used_swap_bytes: u64,
    pub processes: Vec<ProcessSample>,
}

impl MemorySnapshot {
    pub fn used_percent(&self) -> f64 {
        if self.total_memory_bytes == 0 {
            return 0.0;
        }
        (self.used_memory_bytes as f64 / self.total_memory_bytes as f64) * 100.0
    }

    pub fn used_swap_mb(&self) -> u64 {
        self.used_swap_bytes / (1024 * 1024)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ActionPlan {
    pub id: String,
    pub kind: ActionKind,
    pub description: String,
    pub min_level: PressureLevel,
    pub command: Option<String>,
    pub safe_by_default: bool,
    pub priority: u8,
    pub target_pids: Vec<u32>,
    pub rationale: Vec<String>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ActionKind {
    Observe,
    Hook,
    GracefulTerminate,
    HardTerminate,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Decision {
    pub level: PressureLevel,
    pub reasons: Vec<String>,
    pub llm_recommended: bool,
    pub planned_actions: Vec<ActionPlan>,
    pub context_notes: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ContextHints {
    pub source: String,
    pub protected_pids: Vec<u32>,
    pub stale_pids: Vec<u32>,
    pub notes: Vec<String>,
}
