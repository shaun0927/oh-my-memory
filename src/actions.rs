use std::process::Command;

use anyhow::{Context, Result};
use serde::Serialize;
use tracing::{info, warn};

use crate::{
    config::AppConfig,
    models::{ActionKind, ActionPlan, Decision, MemorySnapshot},
};

#[derive(Debug, Clone, Serialize)]
pub struct ExecutionReport {
    pub action_id: String,
    pub executed: bool,
    pub success: bool,
    pub detail: String,
}

pub fn execute_plans(
    config: &AppConfig,
    _snapshot: &MemorySnapshot,
    decision: &Decision,
) -> Vec<ExecutionReport> {
    decision
        .planned_actions
        .iter()
        .map(|plan| execute_plan(config, plan))
        .collect()
}

fn execute_plan(config: &AppConfig, plan: &ActionPlan) -> ExecutionReport {
    if config.actions.dry_run || !config.actions.execute_hooks {
        return ExecutionReport {
            action_id: plan.id.clone(),
            executed: false,
            success: true,
            detail: format!("dry-run: {}", plan.description),
        };
    }

    if plan.command.is_none() {
        return match plan.kind {
            ActionKind::GracefulTerminate => execute_signal_plan("TERM", plan),
            ActionKind::HardTerminate => execute_signal_plan("KILL", plan),
            ActionKind::Observe | ActionKind::Hook => ExecutionReport {
                action_id: plan.id.clone(),
                executed: false,
                success: true,
                detail: "no executable command attached".into(),
            },
        };
    }

    let Some(command) = &plan.command else {
        return ExecutionReport {
            action_id: plan.id.clone(),
            executed: false,
            success: true,
            detail: "no command configured".into(),
        };
    };

    match run_shell(command) {
        Ok(output) => {
            info!(action = %plan.id, "hook executed successfully");
            ExecutionReport {
                action_id: plan.id.clone(),
                executed: true,
                success: true,
                detail: output,
            }
        }
        Err(error) => {
            warn!(action = %plan.id, error = %error, "hook execution failed");
            ExecutionReport {
                action_id: plan.id.clone(),
                executed: true,
                success: false,
                detail: error.to_string(),
            }
        }
    }
}

fn execute_signal_plan(signal: &str, plan: &ActionPlan) -> ExecutionReport {
    if plan.target_pids.is_empty() {
        return ExecutionReport {
            action_id: plan.id.clone(),
            executed: false,
            success: true,
            detail: "no target pids".into(),
        };
    }

    let joined = plan
        .target_pids
        .iter()
        .map(|pid| pid.to_string())
        .collect::<Vec<_>>()
        .join(" ");
    let command = format!("kill -{signal} {joined}");
    match run_shell(&command) {
        Ok(output) => ExecutionReport {
            action_id: plan.id.clone(),
            executed: true,
            success: true,
            detail: if output.is_empty() {
                format!("sent SIG{signal} to {}", joined)
            } else {
                output
            },
        },
        Err(error) => ExecutionReport {
            action_id: plan.id.clone(),
            executed: true,
            success: false,
            detail: error.to_string(),
        },
    }
}

fn run_shell(command: &str) -> Result<String> {
    let output = Command::new("sh")
        .arg("-lc")
        .arg(command)
        .output()
        .with_context(|| format!("failed to spawn hook command: {command}"))?;

    if output.status.success() {
        Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
    } else {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
        anyhow::bail!("hook command failed: {stderr}");
    }
}
