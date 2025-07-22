use log::{error, info, warn};
use reqwest::Client;
use std::time::{Duration, Instant};
use tokio::time::sleep;

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
}

impl Worker {
    pub fn new(settings: Settings) -> Self {
        let client = Client::builder()
            .timeout(Duration::from_secs(30))
            .user_agent(format!("{}/{}", env!("CARGO_PKG_NAME"), env!("CARGO_PKG_VERSION")))
            .build()
            .expect("Failed to create HTTP client");

        Self { client, settings }
    }

    pub async fn start(&self) {
        info!("Worker started with {} monitors", self.settings.monitors.len());
        
        loop {
            self.check_all_monitors().await;
            
            // Calculate sleep duration based on the minimum interval
            let min_interval = self.settings.monitors
                .iter()
                .filter(|monitor| monitor.enabled)
                .map(|monitor| monitor.interval)
                .min()
                .unwrap_or(60);
            
            info!("Worker sleeping for {} seconds", min_interval);
            sleep(Duration::from_secs(min_interval)).await;
        }
    }

    async fn check_all_monitors(&self) {
        let enabled_monitors: Vec<&MonitorConfig> = self.settings.monitors
            .iter()
            .filter(|monitor| monitor.enabled)
            .collect();

        if enabled_monitors.is_empty() {
            warn!("No enabled monitors found");
            return;
        }

        info!("Checking {} enabled monitors", enabled_monitors.len());

        for monitor in enabled_monitors {
            let result = self.check_monitor(monitor).await;
            self.log_result(&result);
        }
    }

    async fn check_monitor(&self, monitor: &MonitorConfig) -> MonitorResult {
        let start_time = Instant::now();
        let timestamp = chrono::Utc::now();
        
        info!("Checking monitor: {} ({})", monitor.name, monitor.url);

        match self.client.get(&monitor.url).send().await {
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
                    error_message: if success { None } else { Some(format!("HTTP {}", status_code)) },
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
                result.status_code.map(|c| c.to_string()).unwrap_or_else(|| "N/A".to_string())
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
        Settings { monitors }
    }

    #[test]
    fn test_worker_new() {
        let settings = create_test_settings(vec![]);
        let worker = Worker::new(settings);
        
        assert_eq!(worker.settings.monitors.len(), 0);
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
        let enabled_monitors: Vec<&MonitorConfig> = settings.monitors
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
}