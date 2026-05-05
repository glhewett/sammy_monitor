use anyhow::Result;
use rusqlite::{params, Connection};

pub struct CheckRecord {
    pub monitor_id: String,
    pub ts: String,
    pub success: bool,
    pub response_time_ms: u64,
    pub status_code: Option<u16>,
    pub error_message: Option<String>,
}

pub struct Db {
    conn: Connection,
}

impl Db {
    pub fn open(path: &str) -> Result<Self> {
        let conn = Connection::open(path)?;
        conn.execute_batch("PRAGMA journal_mode=WAL;")?;
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS monitor_checks (
                id               INTEGER PRIMARY KEY AUTOINCREMENT,
                monitor_id       TEXT    NOT NULL,
                ts               TEXT    NOT NULL,
                success          INTEGER NOT NULL,
                response_time_ms INTEGER NOT NULL,
                status_code      INTEGER,
                error_message    TEXT
            );
            CREATE INDEX IF NOT EXISTS idx_monitor_checks_monitor_id_ts
                ON monitor_checks (monitor_id, ts);",
        )?;
        Ok(Self { conn })
    }

    pub fn insert_check(&self, record: &CheckRecord) -> Result<()> {
        self.conn.execute(
            "INSERT INTO monitor_checks
                (monitor_id, ts, success, response_time_ms, status_code, error_message)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![
                record.monitor_id,
                record.ts,
                record.success as i32,
                record.response_time_ms as i64,
                record.status_code.map(|c| c as i32),
                record.error_message,
            ],
        )?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::NamedTempFile;

    fn open_temp_db() -> (Db, NamedTempFile) {
        let file = NamedTempFile::new().unwrap();
        let db = Db::open(file.path().to_str().unwrap()).unwrap();
        (db, file)
    }

    #[test]
    fn test_open_creates_schema() {
        let (_db, _file) = open_temp_db();
    }

    #[test]
    fn test_insert_success() {
        let (db, _file) = open_temp_db();
        let record = CheckRecord {
            monitor_id: "test-uuid".to_string(),
            ts: "2026-05-03T12:00:00Z".to_string(),
            success: true,
            response_time_ms: 123,
            status_code: Some(200),
            error_message: None,
        };
        db.insert_check(&record).unwrap();
    }

    #[test]
    fn test_insert_failure() {
        let (db, _file) = open_temp_db();
        let record = CheckRecord {
            monitor_id: "test-uuid".to_string(),
            ts: "2026-05-03T12:00:00Z".to_string(),
            success: false,
            response_time_ms: 5000,
            status_code: None,
            error_message: Some("connection refused".to_string()),
        };
        db.insert_check(&record).unwrap();
    }

    #[test]
    fn test_insert_multiple() {
        let (db, _file) = open_temp_db();
        for i in 0..5 {
            let record = CheckRecord {
                monitor_id: format!("uuid-{i}"),
                ts: format!("2026-05-03T12:0{i}:00Z"),
                success: i % 2 == 0,
                response_time_ms: 100 + i * 10,
                status_code: Some(200),
                error_message: None,
            };
            db.insert_check(&record).unwrap();
        }
    }
}
