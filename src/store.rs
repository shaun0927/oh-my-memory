use std::path::Path;

use anyhow::{Context, Result};
use rusqlite::{Connection, OptionalExtension, params};

use crate::{
    actions::ExecutionReport,
    config::AppConfig,
    models::{
        Decision, IncidentDetail, IncidentSummary, MemorySnapshot, PressureLevel, ProcessSample,
    },
};

pub struct Store {
    conn: Connection,
}

#[derive(Debug, Default, Clone)]
pub struct HistoricalProcessStats {
    pub sightings: u32,
    pub stale_hits: u32,
}

impl Store {
    pub fn open(config: &AppConfig) -> Result<Option<Self>> {
        if !config.state.enabled {
            return Ok(None);
        }
        let path = Path::new(&config.state.sqlite_path);
        let conn = Connection::open(path)
            .with_context(|| format!("failed to open sqlite state db: {}", path.display()))?;
        let store = Self { conn };
        store.init()?;
        Ok(Some(store))
    }

    fn init(&self) -> Result<()> {
        self.conn.execute_batch(
            r#"
            PRAGMA journal_mode = WAL;
            CREATE TABLE IF NOT EXISTS incidents (
              id INTEGER PRIMARY KEY AUTOINCREMENT,
              timestamp_unix_secs INTEGER NOT NULL,
              level TEXT NOT NULL,
              used_percent REAL NOT NULL,
              swap_used_mb INTEGER NOT NULL,
              llm_recommended INTEGER NOT NULL,
              snapshot_json TEXT NOT NULL,
              decision_json TEXT NOT NULL,
              llm_output TEXT
            );
            CREATE TABLE IF NOT EXISTS process_samples (
              id INTEGER PRIMARY KEY AUTOINCREMENT,
              incident_id INTEGER NOT NULL,
              pid INTEGER NOT NULL,
              name TEXT NOT NULL,
              command TEXT NOT NULL,
              family TEXT NOT NULL,
              stale_score INTEGER NOT NULL,
              cleanup_candidate INTEGER NOT NULL,
              aggressive_candidate INTEGER NOT NULL,
              runtime_protected INTEGER NOT NULL,
              external_stale_hint INTEGER NOT NULL,
              FOREIGN KEY (incident_id) REFERENCES incidents(id) ON DELETE CASCADE
            );
            CREATE TABLE IF NOT EXISTS action_reports (
              id INTEGER PRIMARY KEY AUTOINCREMENT,
              incident_id INTEGER NOT NULL,
              action_id TEXT NOT NULL,
              executed INTEGER NOT NULL,
              success INTEGER NOT NULL,
              detail TEXT NOT NULL,
              FOREIGN KEY (incident_id) REFERENCES incidents(id) ON DELETE CASCADE
            );
            "#,
        )?;
        Ok(())
    }

    pub fn insert_incident(
        &self,
        snapshot: &MemorySnapshot,
        decision: &Decision,
        reports: &[ExecutionReport],
        llm_output: Option<&str>,
    ) -> Result<i64> {
        let tx = self.conn.unchecked_transaction()?;
        tx.execute(
            "INSERT INTO incidents (timestamp_unix_secs, level, used_percent, swap_used_mb, llm_recommended, snapshot_json, decision_json, llm_output)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
            params![
                snapshot.timestamp_unix_secs as i64,
                decision.level.as_str(),
                snapshot.used_percent(),
                snapshot.used_swap_mb() as i64,
                if decision.llm_recommended { 1 } else { 0 },
                serde_json::to_string(snapshot)?,
                serde_json::to_string(decision)?,
                llm_output,
            ],
        )?;
        let incident_id = tx.last_insert_rowid();

        for process in &snapshot.processes {
            tx.execute(
                "INSERT INTO process_samples (incident_id, pid, name, command, family, stale_score, cleanup_candidate, aggressive_candidate, runtime_protected, external_stale_hint)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
                params![
                    incident_id,
                    process.pid as i64,
                    process.name,
                    process.command,
                    serde_json::to_string(&process.family)?,
                    process.stale_score,
                    if process.cleanup_candidate { 1 } else { 0 },
                    if process.aggressive_candidate { 1 } else { 0 },
                    if process.runtime_protected { 1 } else { 0 },
                    if process.external_stale_hint { 1 } else { 0 },
                ],
            )?;
        }

        for report in reports {
            tx.execute(
                "INSERT INTO action_reports (incident_id, action_id, executed, success, detail)
                 VALUES (?1, ?2, ?3, ?4, ?5)",
                params![
                    incident_id,
                    report.action_id,
                    if report.executed { 1 } else { 0 },
                    if report.success { 1 } else { 0 },
                    report.detail,
                ],
            )?;
        }

        tx.commit()?;
        Ok(incident_id)
    }

    pub fn list_incidents(&self, limit: usize) -> Result<Vec<IncidentSummary>> {
        let mut stmt = self.conn.prepare(
            "SELECT i.id, i.timestamp_unix_secs, i.level, i.used_percent, i.swap_used_mb, i.llm_recommended,
                    (SELECT COUNT(*) FROM action_reports a WHERE a.incident_id = i.id)
             FROM incidents i
             ORDER BY i.id DESC
             LIMIT ?1",
        )?;
        let rows = stmt.query_map(params![limit as i64], |row| {
            Ok(IncidentSummary {
                id: row.get(0)?,
                timestamp_unix_secs: row.get::<_, i64>(1)? as u64,
                level: parse_level(&row.get::<_, String>(2)?),
                used_percent: row.get(3)?,
                swap_used_mb: row.get::<_, i64>(4)? as u64,
                llm_recommended: row.get::<_, i64>(5)? != 0,
                action_count: row.get::<_, i64>(6)? as usize,
            })
        })?;
        Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
    }

    pub fn latest_incident(&self) -> Result<Option<IncidentDetail>> {
        let id = self
            .conn
            .query_row(
                "SELECT id FROM incidents ORDER BY id DESC LIMIT 1",
                [],
                |row| row.get::<_, i64>(0),
            )
            .optional()?;
        match id {
            Some(id) => self.get_incident(id).map(Some),
            None => Ok(None),
        }
    }

    pub fn get_incident(&self, id: i64) -> Result<IncidentDetail> {
        let (summary, snapshot_json, decision_json, llm_output): (IncidentSummary, String, String, Option<String>) = self.conn.query_row(
            "SELECT i.id, i.timestamp_unix_secs, i.level, i.used_percent, i.swap_used_mb, i.llm_recommended,
                    (SELECT COUNT(*) FROM action_reports a WHERE a.incident_id = i.id),
                    i.snapshot_json, i.decision_json, i.llm_output
             FROM incidents i WHERE i.id = ?1",
            params![id],
            |row| {
                Ok((
                    IncidentSummary {
                        id: row.get(0)?,
                        timestamp_unix_secs: row.get::<_, i64>(1)? as u64,
                        level: parse_level(&row.get::<_, String>(2)?),
                        used_percent: row.get(3)?,
                        swap_used_mb: row.get::<_, i64>(4)? as u64,
                        llm_recommended: row.get::<_, i64>(5)? != 0,
                        action_count: row.get::<_, i64>(6)? as usize,
                    },
                    row.get(7)?,
                    row.get(8)?,
                    row.get(9)?,
                ))
            },
        )?;

        let snapshot: MemorySnapshot = serde_json::from_str(&snapshot_json)?;
        let decision: Decision = serde_json::from_str(&decision_json)?;
        let mut stmt = self.conn.prepare(
            "SELECT action_id, executed, success, detail FROM action_reports WHERE incident_id = ?1 ORDER BY id ASC",
        )?;
        let reports = stmt
            .query_map(params![id], |row| {
                Ok(ExecutionReport {
                    action_id: row.get(0)?,
                    executed: row.get::<_, i64>(1)? != 0,
                    success: row.get::<_, i64>(2)? != 0,
                    detail: row.get(3)?,
                })
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;

        Ok(IncidentDetail {
            summary,
            snapshot,
            decision,
            reports,
            llm_output,
        })
    }

    pub fn historical_stats(
        &self,
        samples: &[ProcessSample],
        lookback: usize,
    ) -> Result<std::collections::HashMap<String, HistoricalProcessStats>> {
        let mut map = std::collections::HashMap::new();
        for sample in samples {
            let key = process_history_key(sample);
            let stats = self.conn.query_row(
                "SELECT COUNT(*), COALESCE(SUM(cleanup_candidate), 0)
                 FROM (
                    SELECT cleanup_candidate
                    FROM process_samples
                    WHERE name = ?1 AND family = ?2
                    ORDER BY id DESC
                    LIMIT ?3
                 )",
                params![
                    sample.name,
                    serde_json::to_string(&sample.family)?,
                    lookback as i64
                ],
                |row| {
                    Ok(HistoricalProcessStats {
                        sightings: row.get::<_, i64>(0)? as u32,
                        stale_hits: row.get::<_, i64>(1)? as u32,
                    })
                },
            )?;
            map.insert(key, stats);
        }
        Ok(map)
    }
}

pub fn process_history_key(sample: &ProcessSample) -> String {
    format!("{}::{:?}", sample.name.to_ascii_lowercase(), sample.family)
}

fn parse_level(raw: &str) -> PressureLevel {
    match raw {
        "yellow" => PressureLevel::Yellow,
        "orange" => PressureLevel::Orange,
        "red" => PressureLevel::Red,
        "critical" => PressureLevel::Critical,
        _ => PressureLevel::Green,
    }
}

#[cfg(test)]
mod tests {
    use tempfile::tempdir;

    use crate::{
        actions::ExecutionReport,
        config::AppConfig,
        models::{
            ActionKind, ActionPlan, Decision, Importance, MemorySnapshot, PressureLevel,
            ProcessFamily, ProcessSample,
        },
    };

    fn config(path: &str) -> AppConfig {
        let mut cfg: AppConfig = toml::from_str(AppConfig::default_toml()).expect("default config");
        cfg.state.enabled = true;
        cfg.state.sqlite_path = path.to_string();
        cfg
    }

    #[test]
    fn store_can_round_trip_incident() {
        let dir = tempdir().expect("tempdir");
        let db_path = dir.path().join("state.sqlite3");
        let cfg = config(db_path.to_str().unwrap());
        let store = super::Store::open(&cfg).expect("open").expect("enabled");

        let snapshot = MemorySnapshot {
            timestamp_unix_secs: 123,
            total_memory_bytes: 1000,
            used_memory_bytes: 500,
            available_memory_bytes: 500,
            total_swap_bytes: 100,
            used_swap_bytes: 0,
            processes: vec![ProcessSample {
                pid: 42,
                parent_pid: None,
                name: "playwright".into(),
                command: "playwright".into(),
                memory_bytes: 200,
                cpu_percent: 0.1,
                runtime_secs: 600,
                importance: Importance::Background,
                family: ProcessFamily::BrowserAutomation,
                matched_profile: None,
                parent_missing: true,
                duplicate_family_count: 2,
                recent_activity: false,
                runtime_protected: false,
                protection_reasons: vec![],
                external_stale_hint: false,
                historical_sightings: 0,
                historical_stale_hits: 0,
                stale_score: 75,
                stale_reasons: vec!["parent_missing".into()],
                cleanup_candidate: true,
                aggressive_candidate: false,
            }],
        };

        let decision = Decision {
            level: PressureLevel::Orange,
            reasons: vec!["test".into()],
            llm_recommended: false,
            planned_actions: vec![ActionPlan {
                id: "cleanup_openchrome_workers".into(),
                kind: ActionKind::Hook,
                description: "cleanup".into(),
                min_level: PressureLevel::Orange,
                command: Some("echo cleanup".into()),
                safe_by_default: true,
                priority: 10,
                target_pids: vec![42],
                rationale: vec!["test".into()],
            }],
            context_notes: vec!["tmux active pane".into()],
        };

        let reports = vec![ExecutionReport {
            action_id: "cleanup_openchrome_workers".into(),
            executed: false,
            success: true,
            detail: "dry-run".into(),
        }];

        let id = store
            .insert_incident(&snapshot, &decision, &reports, None)
            .expect("insert");
        let detail = store.get_incident(id).expect("get");
        assert_eq!(detail.summary.id, id);
        assert_eq!(detail.snapshot.processes.len(), 1);
        assert_eq!(detail.reports.len(), 1);
        assert_eq!(detail.decision.level, PressureLevel::Orange);
    }
}
