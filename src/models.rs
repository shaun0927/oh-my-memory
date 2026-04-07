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

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProcessSample {
    pub pid: u32,
    pub name: String,
    pub command: String,
    pub memory_bytes: u64,
    pub cpu_percent: f32,
    pub importance: Importance,
    pub matched_profile: Option<String>,
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
    pub description: String,
    pub min_level: PressureLevel,
    pub command: Option<String>,
    pub safe_by_default: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Decision {
    pub level: PressureLevel,
    pub reasons: Vec<String>,
    pub llm_recommended: bool,
    pub planned_actions: Vec<ActionPlan>,
}
