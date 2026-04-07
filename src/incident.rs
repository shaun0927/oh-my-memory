use anyhow::Result;

use crate::{
    config::AppConfig,
    models::{HealthSummary, IncidentDetail},
    store::Store,
};

pub fn latest(config: &AppConfig) -> Result<Option<IncidentDetail>> {
    let Some(store) = Store::open(config)? else {
        return Ok(None);
    };
    store.latest_incident()
}

pub fn list(config: &AppConfig, limit: usize) -> Result<Vec<crate::models::IncidentSummary>> {
    let Some(store) = Store::open(config)? else {
        return Ok(Vec::new());
    };
    store.list_incidents(limit)
}

pub fn show(config: &AppConfig, id: i64) -> Result<Option<IncidentDetail>> {
    let Some(store) = Store::open(config)? else {
        return Ok(None);
    };
    store.get_incident(id).map(Some)
}

pub fn summarize(config: &AppConfig, limit: usize) -> Result<HealthSummary> {
    let incidents = list(config, limit)?;
    if incidents.is_empty() {
        return Ok(HealthSummary {
            incident_count: 0,
            latest_incident_id: None,
            average_used_percent: 0.0,
            max_used_percent: 0.0,
            average_swap_mb: 0,
            max_swap_mb: 0,
            total_actions: 0,
            llm_recommended_count: 0,
            level_counts: vec![],
        });
    }

    let mut level_counts = std::collections::BTreeMap::<String, usize>::new();
    let mut used_sum = 0.0;
    let mut swap_sum = 0_u64;
    let mut max_used: f64 = 0.0;
    let mut max_swap = 0_u64;
    let mut total_actions = 0_usize;
    let mut llm_count = 0_usize;

    for incident in &incidents {
        *level_counts
            .entry(incident.level.as_str().to_string())
            .or_default() += 1;
        used_sum += incident.used_percent;
        swap_sum += incident.swap_used_mb;
        max_used = max_used.max(incident.used_percent);
        max_swap = max_swap.max(incident.swap_used_mb);
        total_actions += incident.action_count;
        if incident.llm_recommended {
            llm_count += 1;
        }
    }

    Ok(HealthSummary {
        incident_count: incidents.len(),
        latest_incident_id: incidents.first().map(|i| i.id),
        average_used_percent: used_sum / incidents.len() as f64,
        max_used_percent: max_used,
        average_swap_mb: swap_sum / incidents.len() as u64,
        max_swap_mb: max_swap,
        total_actions,
        llm_recommended_count: llm_count,
        level_counts: level_counts.into_iter().collect(),
    })
}
