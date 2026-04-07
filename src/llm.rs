use std::{
    io::Write,
    process::{Command, Stdio},
};

use anyhow::{Context, Result};
use serde_json::json;

use crate::{
    config::AppConfig,
    models::{Decision, MemorySnapshot},
};

pub fn compact_prompt(snapshot: &MemorySnapshot, decision: &Decision) -> String {
    let payload = json!({
        "timestamp": snapshot.timestamp_unix_secs,
        "memory": {
            "used_percent": snapshot.used_percent(),
            "used_swap_mb": snapshot.used_swap_mb(),
            "available_bytes": snapshot.available_memory_bytes,
        },
        "top_processes": snapshot.processes.iter().map(|p| json!({
            "pid": p.pid,
            "name": p.name,
            "memory_bytes": p.memory_bytes,
            "importance": format!("{:?}", p.importance),
            "matched_profile": p.matched_profile,
        })).collect::<Vec<_>>(),
        "decision": {
            "level": decision.level.as_str(),
            "reasons": decision.reasons,
            "planned_actions": decision.planned_actions.iter().map(|a| json!({
                "id": a.id,
                "description": a.description,
                "min_level": a.min_level.as_str(),
            })).collect::<Vec<_>>()
        }
    });

    format!(
        "You are a low-token memory triage advisor. Explain the likely cause of the current memory pressure, justify whether the planned actions are safe, and suggest the safest next step. Keep it compact and operational.\n\n{}",
        serde_json::to_string_pretty(&payload).unwrap_or_else(|_| "{}".to_string())
    )
}

pub fn run_external_analyzer(config: &AppConfig, prompt: &str) -> Result<Option<String>> {
    if !config.llm.enabled || config.llm.external_command.trim().is_empty() {
        return Ok(None);
    }

    let mut child = Command::new("sh")
        .arg("-lc")
        .arg(&config.llm.external_command)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .with_context(|| {
            format!(
                "failed to spawn llm external command: {}",
                config.llm.external_command
            )
        })?;

    if let Some(stdin) = &mut child.stdin {
        stdin
            .write_all(prompt.as_bytes())
            .context("failed to write prompt to llm analyzer stdin")?;
    }

    let output = child
        .wait_with_output()
        .context("failed waiting for llm analyzer")?;
    if output.status.success() {
        Ok(Some(
            String::from_utf8_lossy(&output.stdout).trim().to_string(),
        ))
    } else {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
        anyhow::bail!("llm analyzer failed: {stderr}");
    }
}
