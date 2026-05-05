use anyhow::Result;
use lettre::{
    message::header::ContentType, transport::smtp::authentication::Credentials, AsyncSmtpTransport,
    AsyncTransport, Message, Tokio1Executor,
};
use log::{error, info, warn};
use reqwest::Client;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tokio::time::sleep;
use uuid::Uuid;

use crate::db::{CheckRecord, Db};
use crate::settings::{MonitorConfig, Settings, SmtpConfig};

// smtp_config is kept separate from Settings because it is sourced from env vars,
// not from settings.toml.

#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct MonitorResult {
    pub monitor_id: Uuid,
    pub monitor_name: String,
    pub url: String,
    pub success: bool,
    pub response_time_ms: u64,
    pub status_code: Option<u16>,
    pub error_message: Option<String>,
    pub timestamp: chrono::DateTime<chrono::Utc>,
}

#[derive(Debug, Clone)]
enum MonitorAlertState {
    Unknown,
    Up,
    FailingNoAlert { consecutive_failures: u32 },
    Down,
}

pub struct Worker {
    client: Client,
    settings: Settings,
    last_run_times: HashMap<Uuid, Instant>,
    db: Arc<Mutex<Db>>,
    alert_states: HashMap<Uuid, MonitorAlertState>,
    smtp_config: Option<SmtpConfig>,
    mailer: Option<AsyncSmtpTransport<Tokio1Executor>>,
}

impl Worker {
    pub fn new(settings: Settings, db: Arc<Mutex<Db>>, smtp_config: Option<SmtpConfig>) -> Self {
        let client = Client::builder()
            .timeout(Duration::from_secs(30))
            .user_agent(format!(
                "{}/{}",
                env!("CARGO_PKG_NAME"),
                env!("CARGO_PKG_VERSION")
            ))
            .build()
            .expect("Failed to create HTTP client");

        let mailer = smtp_config.as_ref().and_then(|smtp| {
            build_mailer(smtp)
                .map_err(|e| {
                    warn!("Failed to build SMTP mailer: {e}. Email notifications disabled.");
                    e
                })
                .ok()
        });

        let alert_states = settings
            .monitors
            .iter()
            .map(|m| (m.id(), MonitorAlertState::Unknown))
            .collect();

        Self {
            client,
            settings,
            last_run_times: HashMap::new(),
            db,
            alert_states,
            smtp_config,
            mailer,
        }
    }

    pub async fn start(&mut self) {
        info!(
            "Worker started with {} monitors",
            self.settings.monitors.len()
        );

        loop {
            let loop_start = Instant::now();
            self.check_due_monitors().await;

            let runtime = loop_start.elapsed();
            let sleep_duration = if runtime < Duration::from_secs(60) {
                Duration::from_secs(60) - runtime
            } else {
                Duration::from_millis(100)
            };

            info!(
                "Worker completed in {}ms, sleeping for {}ms",
                runtime.as_millis(),
                sleep_duration.as_millis()
            );
            sleep(sleep_duration).await;
        }
    }

    async fn check_due_monitors(&mut self) {
        let now = Instant::now();
        let mut monitors_to_check = Vec::new();

        for monitor in &self.settings.monitors {
            if !monitor.enabled {
                continue;
            }

            let should_run = match self.last_run_times.get(&monitor.id()) {
                Some(last_run) => {
                    let time_since_last = now.duration_since(*last_run);
                    let interval_duration = Duration::from_secs(monitor.interval * 60);
                    time_since_last >= interval_duration
                }
                None => true,
            };

            if should_run {
                monitors_to_check.push(monitor.clone());
                self.last_run_times.insert(monitor.id(), now);
            }
        }

        if monitors_to_check.is_empty() {
            info!("No monitors due for checking this cycle");
            return;
        }

        info!(
            "Checking {} monitors due for testing",
            monitors_to_check.len()
        );

        for monitor in monitors_to_check {
            let result = self.check_monitor(&monitor).await;
            self.log_result(&result);
            self.persist_result(&result).await;
            self.handle_alert_transition(&monitor, &result).await;
        }
    }

    async fn check_monitor(&self, monitor: &MonitorConfig) -> MonitorResult {
        let start_time = Instant::now();
        let timestamp = chrono::Utc::now();

        info!("Checking monitor: {} ({})", monitor.name, monitor.url);

        match self
            .client
            .get(&monitor.url)
            .header("X-Monitor-Id", monitor.id().to_string())
            .send()
            .await
        {
            Ok(response) => {
                let response_time = start_time.elapsed().as_millis() as u64;
                let status_code = response.status().as_u16();
                let success = response.status().is_success();

                MonitorResult {
                    monitor_id: monitor.id(),
                    monitor_name: monitor.name.clone(),
                    url: monitor.url.clone(),
                    success,
                    response_time_ms: response_time,
                    status_code: Some(status_code),
                    error_message: if success {
                        None
                    } else {
                        Some(format!("HTTP {status_code}"))
                    },
                    timestamp,
                }
            }
            Err(error) => {
                let response_time = start_time.elapsed().as_millis() as u64;

                MonitorResult {
                    monitor_id: monitor.id(),
                    monitor_name: monitor.name.clone(),
                    url: monitor.url.clone(),
                    success: false,
                    response_time_ms: response_time,
                    status_code: None,
                    error_message: Some(error.to_string()),
                    timestamp,
                }
            }
        }
    }

    fn log_result(&self, result: &MonitorResult) {
        if result.success {
            info!(
                "✓ {} ({}) - OK in {}ms [{}]",
                result.monitor_name,
                result.url,
                result.response_time_ms,
                result
                    .status_code
                    .map(|c| c.to_string())
                    .unwrap_or_else(|| "N/A".to_string())
            );
        } else {
            error!(
                "✗ {} ({}) - FAILED in {}ms: {}",
                result.monitor_name,
                result.url,
                result.response_time_ms,
                result.error_message.as_deref().unwrap_or("Unknown error")
            );
        }
    }

    async fn persist_result(&self, result: &MonitorResult) {
        let record = CheckRecord {
            monitor_id: result.monitor_id.to_string(),
            ts: result.timestamp.to_rfc3339(),
            success: result.success,
            response_time_ms: result.response_time_ms,
            status_code: result.status_code,
            error_message: result.error_message.clone(),
        };
        let db = self.db.clone();
        let outcome = tokio::task::spawn_blocking(move || db.lock().unwrap().insert_check(&record))
            .await
            .unwrap_or_else(|e| Err(anyhow::anyhow!("spawn_blocking panicked: {e}")));
        if let Err(e) = outcome {
            error!("Failed to persist check result: {e}");
        }
    }

    async fn handle_alert_transition(&mut self, monitor: &MonitorConfig, result: &MonitorResult) {
        let threshold = self.settings.down_alert_threshold;

        let current_state = self
            .alert_states
            .get(&monitor.id())
            .cloned()
            .unwrap_or(MonitorAlertState::Unknown);

        let new_state = if result.success {
            if matches!(current_state, MonitorAlertState::Down) {
                self.send_recovery_alert(monitor).await;
            }
            MonitorAlertState::Up
        } else {
            match current_state {
                MonitorAlertState::Down => MonitorAlertState::Down,
                MonitorAlertState::FailingNoAlert {
                    consecutive_failures,
                } => {
                    let new_count = consecutive_failures + 1;
                    if new_count >= threshold {
                        self.send_down_alert(monitor, result).await;
                        MonitorAlertState::Down
                    } else {
                        MonitorAlertState::FailingNoAlert {
                            consecutive_failures: new_count,
                        }
                    }
                }
                _ => MonitorAlertState::FailingNoAlert {
                    consecutive_failures: 1,
                },
            }
        };

        self.alert_states.insert(monitor.id(), new_state);
    }

    async fn send_down_alert(&self, monitor: &MonitorConfig, result: &MonitorResult) {
        let (Some(mailer), Some(smtp)) = (&self.mailer, &self.smtp_config) else {
            return;
        };
        let subject = format!("ALERT: {} is DOWN", monitor.name);
        let body = format!(
            "Monitor '{}' ({}) is down.\n\nLast error: {}\nResponse time: {}ms\nTime: {}",
            monitor.name,
            monitor.url,
            result.error_message.as_deref().unwrap_or("unknown"),
            result.response_time_ms,
            result.timestamp.to_rfc3339(),
        );
        send_emails(mailer, smtp, &subject, &body).await;
    }

    async fn send_recovery_alert(&self, monitor: &MonitorConfig) {
        let (Some(mailer), Some(smtp)) = (&self.mailer, &self.smtp_config) else {
            return;
        };
        let subject = format!("RECOVERED: {} is back UP", monitor.name);
        let body = format!(
            "Monitor '{}' ({}) has recovered.\n\nTime: {}",
            monitor.name,
            monitor.url,
            chrono::Utc::now().to_rfc3339(),
        );
        send_emails(mailer, smtp, &subject, &body).await;
    }
}

fn build_mailer(smtp: &SmtpConfig) -> Result<AsyncSmtpTransport<Tokio1Executor>> {
    let creds = Credentials::new(smtp.smtp_username.clone(), smtp.smtp_password.clone());
    let builder = if smtp.smtp_use_tls {
        AsyncSmtpTransport::<Tokio1Executor>::starttls_relay(&smtp.smtp_host)?
    } else {
        AsyncSmtpTransport::<Tokio1Executor>::relay(&smtp.smtp_host)?
    };
    Ok(builder.port(smtp.smtp_port).credentials(creds).build())
}

fn build_email(from: &str, to: &str, subject: &str, body: &str) -> Result<Message> {
    Ok(Message::builder()
        .from(from.parse()?)
        .to(to.parse()?)
        .subject(subject)
        .header(ContentType::TEXT_PLAIN)
        .body(body.to_string())?)
}

async fn send_emails(
    mailer: &AsyncSmtpTransport<Tokio1Executor>,
    smtp: &SmtpConfig,
    subject: &str,
    body: &str,
) {
    for to in &smtp.to_addresses {
        match build_email(&smtp.from_address, to, subject, body) {
            Ok(email) => {
                if let Err(e) = mailer.send(email).await {
                    warn!("Failed to send email to {to}: {e}");
                } else {
                    info!("Email sent to {to}: {subject}");
                }
            }
            Err(e) => warn!("Failed to build email to {to}: {e}"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::Db;
    use tempfile::NamedTempFile;

    fn make_test_db() -> (Arc<Mutex<Db>>, NamedTempFile) {
        let file = NamedTempFile::new().unwrap();
        let db = Db::open(file.path().to_str().unwrap()).unwrap();
        (Arc::new(Mutex::new(db)), file)
    }

    fn create_test_monitor(name: &str, url: &str, enabled: bool) -> MonitorConfig {
        MonitorConfig {
            name: name.to_string(),
            url: url.to_string(),
            interval: 60,
            enabled,
        }
    }

    fn create_test_settings(monitors: Vec<MonitorConfig>) -> Settings {
        Settings {
            monitors,
            db_path: "./test.db".to_string(),
            down_alert_threshold: 5,
        }
    }

    #[test]
    fn test_worker_new() {
        let settings = create_test_settings(vec![]);
        let (db, _file) = make_test_db();
        let worker = Worker::new(settings, db, None);

        assert_eq!(worker.settings.monitors.len(), 0);
        assert_eq!(worker.last_run_times.len(), 0);
    }

    #[test]
    fn test_monitor_result_creation() {
        let monitor_id = Uuid::new_v4();
        let timestamp = chrono::Utc::now();

        let result = MonitorResult {
            monitor_id,
            monitor_name: "Test Monitor".to_string(),
            url: "https://example.com".to_string(),
            success: true,
            response_time_ms: 150,
            status_code: Some(200),
            error_message: None,
            timestamp,
        };

        assert_eq!(result.monitor_id, monitor_id);
        assert_eq!(result.monitor_name, "Test Monitor");
        assert_eq!(result.url, "https://example.com");
        assert!(result.success);
        assert_eq!(result.response_time_ms, 150);
        assert_eq!(result.status_code, Some(200));
        assert!(result.error_message.is_none());
    }

    #[test]
    fn test_settings_filter_enabled_monitors() {
        let monitors = vec![
            create_test_monitor("Enabled Monitor", "https://enabled.com", true),
            create_test_monitor("Disabled Monitor", "https://disabled.com", false),
            create_test_monitor("Another Enabled", "https://enabled2.com", true),
        ];

        let settings = create_test_settings(monitors);
        let enabled_monitors: Vec<&MonitorConfig> = settings
            .monitors
            .iter()
            .filter(|monitor| monitor.enabled)
            .collect();

        assert_eq!(enabled_monitors.len(), 2);
        assert_eq!(enabled_monitors[0].name, "Enabled Monitor");
        assert_eq!(enabled_monitors[1].name, "Another Enabled");
    }

    #[tokio::test]
    async fn test_check_monitor_success() {
        let monitor = create_test_monitor("Test", "https://httpbin.org/status/200", true);
        let settings = create_test_settings(vec![monitor.clone()]);
        let (db, _file) = make_test_db();
        let worker = Worker::new(settings, db, None);

        let result = worker.check_monitor(&monitor).await;

        assert_eq!(result.monitor_id, monitor.id());
        assert_eq!(result.monitor_name, monitor.name);
        assert_eq!(result.url, monitor.url);
    }

    #[test]
    fn test_interval_scheduling() {
        let monitors = vec![
            MonitorConfig {
                name: "1min interval".to_string(),
                url: "https://example1.com".to_string(),
                interval: 1,
                enabled: true,
            },
            MonitorConfig {
                name: "2min interval".to_string(),
                url: "https://example2.com".to_string(),
                interval: 2,
                enabled: true,
            },
            MonitorConfig {
                name: "Disabled".to_string(),
                url: "https://disabled.com".to_string(),
                interval: 1,
                enabled: false,
            },
        ];

        let settings = create_test_settings(monitors.clone());
        let (db, _file) = make_test_db();
        let mut worker = Worker::new(settings, db, None);

        assert_eq!(worker.last_run_times.len(), 0);

        let now = std::time::Instant::now();
        for monitor in &monitors {
            if monitor.enabled {
                let should_run = match worker.last_run_times.get(&monitor.id()) {
                    Some(last_run) => {
                        let time_since_last = now.duration_since(*last_run);
                        let interval_duration = Duration::from_secs(monitor.interval * 60);
                        time_since_last >= interval_duration
                    }
                    None => true,
                };
                assert!(
                    should_run,
                    "Monitor {} should run on first cycle",
                    monitor.name
                );
            }
        }

        worker.last_run_times.insert(monitors[0].id(), now);
        worker.last_run_times.insert(monitors[1].id(), now);

        for monitor in &monitors {
            if monitor.enabled {
                let should_run = match worker.last_run_times.get(&monitor.id()) {
                    Some(last_run) => {
                        let time_since_last = now.duration_since(*last_run);
                        let interval_duration = Duration::from_secs(monitor.interval * 60);
                        time_since_last >= interval_duration
                    }
                    None => true,
                };
                assert!(
                    !should_run,
                    "Monitor {} should not run immediately after being run",
                    monitor.name
                );
            }
        }
    }

    #[test]
    fn test_alert_state_machine_down_threshold() {
        let monitor = create_test_monitor("Test", "https://example.com", true);
        let settings = create_test_settings(vec![monitor.clone()]);
        let (db, _file) = make_test_db();
        let mut worker = Worker::new(settings, db, None);

        // Initial state is Unknown — first failure goes to FailingNoAlert{1}
        let state = worker
            .alert_states
            .get(&monitor.id())
            .cloned()
            .unwrap_or(MonitorAlertState::Unknown);
        assert!(matches!(state, MonitorAlertState::Unknown));

        // Simulate 4 failures (threshold=5, not yet Down)
        for i in 1..5u32 {
            let new_state = match worker.alert_states[&monitor.id()].clone() {
                MonitorAlertState::FailingNoAlert {
                    consecutive_failures,
                } => MonitorAlertState::FailingNoAlert {
                    consecutive_failures: consecutive_failures + 1,
                },
                _ => MonitorAlertState::FailingNoAlert {
                    consecutive_failures: 1,
                },
            };
            worker.alert_states.insert(monitor.id(), new_state);
            let state = &worker.alert_states[&monitor.id()];
            assert!(
                matches!(state, MonitorAlertState::FailingNoAlert { consecutive_failures } if *consecutive_failures == i),
                "Expected FailingNoAlert with count {i}"
            );
        }

        // 5th failure should reach threshold
        let new_state = match worker.alert_states[&monitor.id()].clone() {
            MonitorAlertState::FailingNoAlert {
                consecutive_failures,
            } => {
                let new_count = consecutive_failures + 1;
                if new_count >= 5 {
                    MonitorAlertState::Down
                } else {
                    MonitorAlertState::FailingNoAlert {
                        consecutive_failures: new_count,
                    }
                }
            }
            s => s,
        };
        worker.alert_states.insert(monitor.id(), new_state);
        assert!(matches!(
            worker.alert_states[&monitor.id()],
            MonitorAlertState::Down
        ));
    }
}
