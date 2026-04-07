use std::collections::HashMap;

use crate::{
    config::AppConfig,
    models::ProcessSample,
    store::{HistoricalProcessStats, process_history_key},
};

pub fn apply_historical_stats(
    config: &AppConfig,
    processes: &mut [ProcessSample],
    stats: &HashMap<String, HistoricalProcessStats>,
) {
    for process in processes {
        let key = process_history_key(process);
        let Some(stat) = stats.get(&key) else {
            continue;
        };
        process.historical_sightings = stat.sightings;
        process.historical_stale_hits = stat.stale_hits;

        if stat.sightings >= config.state.stale_history_bonus_threshold {
            process.stale_score += 10;
            process
                .stale_reasons
                .push(format!("historical_sightings={}", stat.sightings));
        }
        if stat.stale_hits >= config.state.stale_history_bonus_threshold {
            process.stale_score += 10;
            process
                .stale_reasons
                .push(format!("historical_stale_hits={}", stat.stale_hits));
        }
    }
}
