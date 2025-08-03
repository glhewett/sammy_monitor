use log::{error, info};
use reqwest::Client;
use std::collections::HashMap;
use std::time::{Duration, Instant};
use tokio::time::sleep;
use uuid::Uuid;

use crate::metrics::{MonitorMetadata, METRICS_REGISTRY};
use crate::settings::{MonitorConfig, Settings};

#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct MonitorResult {
    pub monitor_id: uuid::Uuid,
    pub monitor_name: String,
    pub url: String,
    pub success: bool,
    pub response_time_ms: u64,
    pub status_code: Option<u16>,
    pub error_message: Option<String>,
    pub timestamp: chrono::DateTime<chrono::Utc>,
}

pub struct Worker {
    client: Client,
    settings: Settings,
    last_run_times: HashMap<Uuid, Instant>,
}

impl Worker {
    pub fn new(settings: Settings) -> Self {
        let client = Client::builder()
            .timeout(Duration::from_secs(30))
            .user_agent(format!(
                "{}/{}",
                env!("CARGO_PKG_NAME"),
                env!("CARGO_PKG_VERSION")
            ))
            .build()
            .expect("Failed to create HTTP client");

        // Register all monitors with metrics registry
        for monitor in &settings.monitors {
            let metadata = MonitorMetadata {
                name: monitor.name.clone(),
                url: monitor.url.clone(),
                interval: monitor.interval,
            };
            METRICS_REGISTRY.register_monitor(monitor.id, metadata);
        }

        Self {
            client,
            settings,
            last_run_times: HashMap::new(),
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

            // Sleep for 1 minute minus the runtime
            let runtime = loop_start.elapsed();
            let sleep_duration = if runtime < Duration::from_secs(60) {
                Duration::from_secs(60) - runtime
            } else {
                Duration::from_millis(100) // Minimum sleep to prevent busy loop
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

            let should_run = match self.last_run_times.get(&monitor.id) {
                Some(last_run) => {
                    let time_since_last = now.duration_since(*last_run);
                    let interval_duration = Duration::from_secs(monitor.interval * 60); // Convert minutes to seconds
                    time_since_last >= interval_duration
                }
                None => true, // First run
            };

            if should_run {
                monitors_to_check.push(monitor);
                self.last_run_times.insert(monitor.id, now);
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
            let result = self.check_monitor(monitor).await;
            self.log_result(&result);
            self.record_metrics(&result);
        }
    }

    async fn check_monitor(&self, monitor: &MonitorConfig) -> MonitorResult {
        let start_time = Instant::now();
        let timestamp = chrono::Utc::now();

        info!("Checking monitor: {} ({})", monitor.name, monitor.url);

        match self
            .client
            .get(&monitor.url)
            .header("X-Monitor-Id", monitor.id.to_string())
            .send()
            .await
        {
            Ok(response) => {
                let response_time = start_time.elapsed().as_millis() as u64;
                let status_code = response.status().as_u16();
                let success = response.status().is_success();

                MonitorResult {
                    monitor_id: monitor.id,
                    monitor_name: monitor.name.clone(),
                    url: monitor.url.clone(),
                    success,
                    response_time_ms: response_time,
                    status_code: Some(status_code),
                    error_message: if success {
                        None
                    } else {
                        Some(format!("HTTP {}", status_code))
                    },
                    timestamp,
                }
            }
            Err(error) => {
                let response_time = start_time.elapsed().as_millis() as u64;

                MonitorResult {
                    monitor_id: monitor.id,
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

    fn record_metrics(&self, result: &MonitorResult) {
        if result.success {
            METRICS_REGISTRY.record_success(result.monitor_id, result.response_time_ms);
        } else {
            // Determine error type from the error message
            let error_type = if result
                .error_message
                .as_ref()
                .map(|msg| msg.contains("timeout"))
                .unwrap_or(false)
            {
                "timeout"
            } else if result.status_code.is_some() {
                "http_error"
            } else if result
                .error_message
                .as_ref()
                .map(|msg| msg.contains("dns"))
                .unwrap_or(false)
            {
                "dns_error"
            } else {
                "connection_error"
            };

            METRICS_REGISTRY.record_failure(
                result.monitor_id,
                result.response_time_ms,
                error_type,
                result.status_code,
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use uuid::Uuid;

    fn create_test_monitor(name: &str, url: &str, enabled: bool) -> MonitorConfig {
        MonitorConfig {
            id: Uuid::new_v4(),
            name: name.to_string(),
            url: url.to_string(),
            interval: 60,
            enabled,
        }
    }

    fn create_test_settings(monitors: Vec<MonitorConfig>) -> Settings {
        Settings {
            prometheus_url: "http://foo:9090",
            monitors,
        }
    }

    #[test]
    fn test_worker_new() {
        let settings = create_test_settings(vec![]);
        let worker = Worker::new(settings);

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
        // This test would require a mock HTTP server in a real implementation
        // For now, we just test the structure
        let monitor = create_test_monitor("Test", "https://httpbin.org/status/200", true);
        let settings = create_test_settings(vec![monitor.clone()]);
        let worker = Worker::new(settings);

        let result = worker.check_monitor(&monitor).await;

        assert_eq!(result.monitor_id, monitor.id);
        assert_eq!(result.monitor_name, monitor.name);
        assert_eq!(result.url, monitor.url);
        // Note: This test will actually make an HTTP request
        // In production, you'd want to mock the HTTP client
    }

    #[test]
    fn test_interval_scheduling() {
        let monitors = vec![
            MonitorConfig {
                id: Uuid::new_v4(),
                name: "1min interval".to_string(),
                url: "https://example1.com".to_string(),
                interval: 1, // 1 minute
                enabled: true,
            },
            MonitorConfig {
                id: Uuid::new_v4(),
                name: "2min interval".to_string(),
                url: "https://example2.com".to_string(),
                interval: 2, // 2 minutes
                enabled: true,
            },
            MonitorConfig {
                id: Uuid::new_v4(),
                name: "Disabled".to_string(),
                url: "https://disabled.com".to_string(),
                interval: 1,
                enabled: false,
            },
        ];

        let settings = create_test_settings(monitors.clone());
        let mut worker = Worker::new(settings);

        // Initially, no monitors have been run
        assert_eq!(worker.last_run_times.len(), 0);

        // Simulate a first run - all enabled monitors should be due
        let now = std::time::Instant::now();
        for monitor in &monitors {
            if monitor.enabled {
                let should_run = match worker.last_run_times.get(&monitor.id) {
                    Some(last_run) => {
                        let time_since_last = now.duration_since(*last_run);
                        let interval_duration = Duration::from_secs(monitor.interval * 60);
                        time_since_last >= interval_duration
                    }
                    None => true, // First run
                };
                assert!(
                    should_run,
                    "Monitor {} should run on first cycle",
                    monitor.name
                );
            }
        }

        // Mark monitors as run
        worker.last_run_times.insert(monitors[0].id, now);
        worker.last_run_times.insert(monitors[1].id, now);

        // Immediately after running, no monitors should be due
        for monitor in &monitors {
            if monitor.enabled {
                let should_run = match worker.last_run_times.get(&monitor.id) {
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
}
