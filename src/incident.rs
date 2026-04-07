use anyhow::Result;

use crate::{config::AppConfig, models::IncidentDetail, store::Store};

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
