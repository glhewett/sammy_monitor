use sammy_monitor::db::{CheckRecord, Db};
use tempfile::NamedTempFile;

#[test]
fn test_db_integration_insert_and_schema() {
    let file = NamedTempFile::new().unwrap();
    let db = Db::open(file.path().to_str().unwrap()).unwrap();

    let records = vec![
        CheckRecord {
            monitor_id: "monitor-1".to_string(),
            ts: "2026-05-03T10:00:00Z".to_string(),
            success: true,
            response_time_ms: 120,
            status_code: Some(200),
            error_message: None,
        },
        CheckRecord {
            monitor_id: "monitor-1".to_string(),
            ts: "2026-05-03T10:01:00Z".to_string(),
            success: false,
            response_time_ms: 5000,
            status_code: None,
            error_message: Some("connection refused".to_string()),
        },
        CheckRecord {
            monitor_id: "monitor-2".to_string(),
            ts: "2026-05-03T10:00:30Z".to_string(),
            success: true,
            response_time_ms: 88,
            status_code: Some(200),
            error_message: None,
        },
    ];

    for record in &records {
        db.insert_check(record).unwrap();
    }
}

#[test]
fn test_db_reopens_existing_schema() {
    let file = NamedTempFile::new().unwrap();
    let path = file.path().to_str().unwrap().to_string();

    {
        let db = Db::open(&path).unwrap();
        db.insert_check(&CheckRecord {
            monitor_id: "monitor-1".to_string(),
            ts: "2026-05-03T10:00:00Z".to_string(),
            success: true,
            response_time_ms: 100,
            status_code: Some(200),
            error_message: None,
        })
        .unwrap();
    }

    // Reopening should not fail or recreate the schema destructively
    let db = Db::open(&path).unwrap();
    db.insert_check(&CheckRecord {
        monitor_id: "monitor-1".to_string(),
        ts: "2026-05-03T10:01:00Z".to_string(),
        success: true,
        response_time_ms: 95,
        status_code: Some(200),
        error_message: None,
    })
    .unwrap();
}
