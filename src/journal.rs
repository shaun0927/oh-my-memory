use std::{
    fs::{self, OpenOptions},
    io::Write,
    path::{Path, PathBuf},
};

use anyhow::{Context, Result};
use serde_json::json;

use crate::{
    actions::ExecutionReport,
    config::AppConfig,
    models::{Decision, MemorySnapshot},
};

fn state_dir(config: &AppConfig) -> PathBuf {
    PathBuf::from(&config.journal.directory)
}

pub fn ensure_state_dirs(config: &AppConfig) -> Result<()> {
    fs::create_dir_all(state_dir(config)).with_context(|| "failed to create journal directory")?;
    Ok(())
}

pub fn write_latest_snapshot(
    config: &AppConfig,
    snapshot: &MemorySnapshot,
    decision: &Decision,
) -> Result<PathBuf> {
    ensure_state_dirs(config)?;
    let path = state_dir(config).join("latest.json");
    let data = json!({
        "snapshot": snapshot,
        "decision": decision,
    });
    fs::write(&path, serde_json::to_vec_pretty(&data)?)
        .with_context(|| format!("failed writing {}", path.display()))?;
    Ok(path)
}

pub fn append_journal_entry(
    config: &AppConfig,
    snapshot: &MemorySnapshot,
    decision: &Decision,
    reports: &[ExecutionReport],
    llm_output: Option<&str>,
) -> Result<PathBuf> {
    ensure_state_dirs(config)?;
    let path = state_dir(config).join("journal.jsonl");
    let mut file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(&path)
        .with_context(|| format!("failed opening {}", path.display()))?;

    let line = json!({
        "snapshot": snapshot,
        "decision": decision,
        "execution_reports": reports,
        "llm_output": llm_output,
    });
    writeln!(file, "{}", serde_json::to_string(&line)?)
        .with_context(|| format!("failed appending {}", path.display()))?;
    Ok(path)
}

pub fn latest_snapshot_path(config: &AppConfig) -> PathBuf {
    state_dir(config).join("latest.json")
}

pub fn read_latest_snapshot(path: &Path) -> Result<String> {
    let raw =
        fs::read_to_string(path).with_context(|| format!("failed reading {}", path.display()))?;
    Ok(raw)
}
