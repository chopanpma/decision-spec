//! Run history for DecisionSpec, stored in a local SQLite database
//! (`.decispec/decispec.db`). The gate records every run; `decispec query`
//! reads the latest per-decision outcome back.

use std::path::Path;

use rusqlite::{Connection, OptionalExtension, params};
use serde::{Deserialize, Serialize};

#[derive(Debug, thiserror::Error)]
pub enum StoreError {
    #[error("sqlite error: {0}")]
    Sqlite(#[from] rusqlite::Error),
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
}

/// Per-decision outcome recorded with a run.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DecisionRun {
    pub decision_id: String,
    pub outcome: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RunRecord {
    /// UTC timestamp, RFC3339-ish `YYYY-MM-DDTHH:MM:SSZ`.
    pub ts: String,
    pub verdict: String,
    pub total_rows: usize,
    pub passed: usize,
    pub failed: usize,
    pub uncovered: usize,
    pub skipped: usize,
    pub report_json: String,
    pub decisions: Vec<DecisionRun>,
}

#[derive(Debug)]
pub struct Store {
    conn: Connection,
}

impl Store {
    pub fn open(path: &Path) -> Result<Self, StoreError> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let conn = Connection::open(path)?;
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS runs (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                ts TEXT NOT NULL,
                verdict TEXT NOT NULL,
                total_rows INTEGER NOT NULL,
                passed INTEGER NOT NULL,
                failed INTEGER NOT NULL,
                uncovered INTEGER NOT NULL,
                skipped INTEGER NOT NULL,
                report_json TEXT NOT NULL
            );
            CREATE TABLE IF NOT EXISTS decision_results (
                run_id INTEGER NOT NULL REFERENCES runs(id),
                decision_id TEXT NOT NULL,
                outcome TEXT NOT NULL,
                PRIMARY KEY (run_id, decision_id)
            );",
        )?;
        Ok(Store { conn })
    }

    pub fn record_run(&self, record: &RunRecord) -> Result<i64, StoreError> {
        let tx = self.conn.unchecked_transaction()?;
        tx.execute(
            "INSERT INTO runs (ts, verdict, total_rows, passed, failed, uncovered, skipped, report_json)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
            params![
                record.ts,
                record.verdict,
                record.total_rows,
                record.passed,
                record.failed,
                record.uncovered,
                record.skipped,
                record.report_json,
            ],
        )?;
        let run_id = tx.last_insert_rowid();
        for d in &record.decisions {
            tx.execute(
                "INSERT INTO decision_results (run_id, decision_id, outcome) VALUES (?1, ?2, ?3)",
                params![run_id, d.decision_id, d.outcome],
            )?;
        }
        tx.commit()?;
        Ok(run_id)
    }

    /// Outcome of `decision_id` in the most recent run that covered it.
    pub fn latest_decision_outcome(
        &self,
        decision_id: &str,
    ) -> Result<Option<(String, String)>, StoreError> {
        let row: Option<(String, String)> = self
            .conn
            .query_row(
                "SELECT r.verdict, r.ts
                 FROM decision_results d JOIN runs r ON r.id = d.run_id
                 WHERE d.decision_id = ?1
                 ORDER BY r.id DESC LIMIT 1",
                params![decision_id],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .optional()?;
        Ok(row)
    }

    pub fn run_count(&self) -> Result<i64, StoreError> {
        let n: i64 = self
            .conn
            .query_row("SELECT COUNT(*) FROM runs", [], |row| row.get(0))?;
        Ok(n)
    }
}

/// Current UTC time formatted as `YYYY-MM-DDTHH:MM:SSZ` (no chrono dependency).
pub fn now_utc() -> String {
    let secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0) as i64;
    let days = secs.div_euclid(86400);
    let tod = secs.rem_euclid(86400);
    let (h, mi, s) = (tod / 3600, (tod % 3600) / 60, tod % 60);
    // Howard Hinnant's civil_from_days.
    let z = days + 719468;
    let era = z.div_euclid(146097);
    let doe = z.rem_euclid(146097);
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if m <= 2 { y + 1 } else { y };
    format!("{y:04}-{m:02}-{d:02}T{h:02}:{mi:02}:{s:02}Z")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_db(name: &str) -> std::path::PathBuf {
        let path = std::env::temp_dir().join(format!(
            "decispec-store-test-{name}-{}-{}.db",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let _ = std::fs::remove_file(&path);
        path
    }

    fn sample_run(verdict: &str) -> RunRecord {
        RunRecord {
            ts: now_utc(),
            verdict: verdict.to_string(),
            total_rows: 6,
            passed: 3,
            failed: 2,
            uncovered: 1,
            skipped: 0,
            report_json: "{\"verdict\":\"fail\"}".to_string(),
            decisions: vec![
                DecisionRun {
                    decision_id: "AUTH-001".to_string(),
                    outcome: "fail".to_string(),
                },
                DecisionRun {
                    decision_id: "AUTH-007".to_string(),
                    outcome: "pass".to_string(),
                },
            ],
        }
    }

    #[test]
    fn record_and_read_back() {
        let path = temp_db("roundtrip");
        {
            let store = Store::open(&path).unwrap();
            assert_eq!(store.run_count().unwrap(), 0);
            let id1 = store.record_run(&sample_run("fail")).unwrap();
            let id2 = store.record_run(&sample_run("pass")).unwrap();
            assert!(id2 > id1);
            assert_eq!(store.run_count().unwrap(), 2);

            let (verdict, _ts) = store
                .latest_decision_outcome("AUTH-001")
                .unwrap()
                .expect("AUTH-001 should have an outcome");
            assert_eq!(verdict, "pass", "latest run wins");
            let (verdict7, _) = store
                .latest_decision_outcome("AUTH-007")
                .unwrap()
                .expect("AUTH-007 should have an outcome");
            assert_eq!(verdict7, "pass");
            assert!(store.latest_decision_outcome("NOPE-1").unwrap().is_none());
        }
        std::fs::remove_file(&path).unwrap();
    }

    #[test]
    fn opening_twice_reuses_schema() {
        let path = temp_db("twice");
        {
            let store = Store::open(&path).unwrap();
            store.record_run(&sample_run("pass")).unwrap();
        }
        {
            let store = Store::open(&path).unwrap();
            assert_eq!(store.run_count().unwrap(), 1);
        }
        std::fs::remove_file(&path).unwrap();
    }

    #[test]
    fn now_utc_formats_rfc3339ish() {
        let ts = now_utc();
        assert_eq!(ts.len(), 20);
        assert!(ts.ends_with('Z'));
        assert_eq!(&ts[4..5], "-");
        assert_eq!(&ts[10..11], "T");
    }
}
