use metrics::{Counter, Gauge, Histogram, Unit};
use once_cell::sync::Lazy;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use uuid::Uuid;

/// Shared metrics registry that can be accessed by both worker and metrics endpoint
pub static METRICS_REGISTRY: Lazy<Arc<MetricsRegistry>> =
    Lazy::new(|| Arc::new(MetricsRegistry::new()));

/// Central metrics registry for HTTP monitoring
pub struct MetricsRegistry {
    /// Response time histograms per monitor
    /// Buckets: 50ms, 100ms, 200ms, 500ms, 1s, 2s, 5s, 10s, +Inf
    response_time_histograms: Mutex<HashMap<Uuid, Histogram>>,

    /// Total request counters per monitor and status
    request_counters: Mutex<HashMap<String, Counter>>,

    /// Failure counters with error type classification
    failure_counters: Mutex<HashMap<String, Counter>>,

    /// Current monitor status (1.0 = up, 0.0 = down)
    monitor_status_gauges: Mutex<HashMap<Uuid, Gauge>>,

    /// Timestamp of last successful check per monitor
    last_success_timestamps: Mutex<HashMap<Uuid, Gauge>>,

    /// Monitor metadata for labels
    monitor_metadata: Mutex<HashMap<Uuid, MonitorMetadata>>,
}

#[derive(Debug, Clone)]
pub struct MonitorMetadata {
    pub name: String,
    pub url: String,
    pub interval: u64,
}

impl MetricsRegistry {
    pub fn new() -> Self {
        Self {
            response_time_histograms: Mutex::new(HashMap::new()),
            request_counters: Mutex::new(HashMap::new()),
            failure_counters: Mutex::new(HashMap::new()),
            monitor_status_gauges: Mutex::new(HashMap::new()),
            last_success_timestamps: Mutex::new(HashMap::new()),
            monitor_metadata: Mutex::new(HashMap::new()),
        }
    }

    /// Register a new monitor for metrics tracking
    pub fn register_monitor(&self, id: Uuid, metadata: MonitorMetadata) {
        let mut meta_map = self.monitor_metadata.lock().unwrap();
        meta_map.insert(id, metadata.clone());
        drop(meta_map);

        // Initialize response time histogram with appropriate buckets
        let mut histograms = self.response_time_histograms.lock().unwrap();
        let histogram = metrics::histogram!(
            "http_monitor_response_time_seconds",
            "monitor_id" => id.to_string(),
            "monitor_name" => metadata.name.clone(),
            "monitor_url" => metadata.url.clone(),
            "interval_minutes" => metadata.interval.to_string()
        );
        histograms.insert(id, histogram);
        drop(histograms);

        // Initialize request counters for success/failure
        let mut counters = self.request_counters.lock().unwrap();
        let success_key = format!("{}:success", id);
        let failure_key = format!("{}:failure", id);

        counters.insert(
            success_key.clone(),
            metrics::counter!(
                "http_monitor_requests_total",
                "monitor_id" => id.to_string(),
                "monitor_name" => metadata.name.clone(),
                "monitor_url" => metadata.url.clone(),
                "interval_minutes" => metadata.interval.to_string(),
                "status" => "success"
            ),
        );

        counters.insert(
            failure_key.clone(),
            metrics::counter!(
                "http_monitor_requests_total",
                "monitor_id" => id.to_string(),
                "monitor_name" => metadata.name.clone(),
                "monitor_url" => metadata.url.clone(),
                "interval_minutes" => metadata.interval.to_string(),
                "status" => "failure"
            ),
        );
        drop(counters);

        // Initialize status gauge
        let mut gauges = self.monitor_status_gauges.lock().unwrap();
        gauges.insert(
            id,
            metrics::gauge!(
                "http_monitor_up",
                "monitor_id" => id.to_string(),
                "monitor_name" => metadata.name.clone(),
                "monitor_url" => metadata.url.clone(),
                "interval_minutes" => metadata.interval.to_string()
            ),
        );
        drop(gauges);

        // Initialize last success timestamp
        let mut timestamps = self.last_success_timestamps.lock().unwrap();
        timestamps.insert(
            id,
            metrics::gauge!(
                "http_monitor_last_success_timestamp",
                "monitor_id" => id.to_string(),
                "monitor_name" => metadata.name.clone(),
                "monitor_url" => metadata.url.clone(),
                "interval_minutes" => metadata.interval.to_string()
            ),
        );
    }

    /// Record a successful HTTP check
    pub fn record_success(&self, monitor_id: Uuid, response_time_ms: u64) {
        // Record response time in histogram (convert ms to seconds)
        if let Ok(histograms) = self.response_time_histograms.lock() {
            if let Some(histogram) = histograms.get(&monitor_id) {
                histogram.record(response_time_ms as f64 / 1000.0);
            }
        }

        // Increment success counter
        if let Ok(counters) = self.request_counters.lock() {
            let success_key = format!("{}:success", monitor_id);
            if let Some(counter) = counters.get(&success_key) {
                counter.increment(1);
            }
        }

        // Update status to up (1.0)
        if let Ok(gauges) = self.monitor_status_gauges.lock() {
            if let Some(gauge) = gauges.get(&monitor_id) {
                gauge.set(1.0);
            }
        }

        // Update last success timestamp
        if let Ok(timestamps) = self.last_success_timestamps.lock() {
            if let Some(timestamp) = timestamps.get(&monitor_id) {
                let now = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap()
                    .as_secs() as f64;
                timestamp.set(now);
            }
        }
    }

    /// Record a failed HTTP check
    pub fn record_failure(
        &self,
        monitor_id: Uuid,
        response_time_ms: u64,
        error_type: &str,
        status_code: Option<u16>,
    ) {
        // Still record response time for failed requests (important for timeout analysis)
        if let Ok(histograms) = self.response_time_histograms.lock() {
            if let Some(histogram) = histograms.get(&monitor_id) {
                histogram.record(response_time_ms as f64 / 1000.0);
            }
        }

        // Increment failure counter
        if let Ok(counters) = self.request_counters.lock() {
            let failure_key = format!("{}:failure", monitor_id);
            if let Some(counter) = counters.get(&failure_key) {
                counter.increment(1);
            }
        }

        // Record specific failure type counter
        if let Ok(metadata) = self.monitor_metadata.lock() {
            if let Some(meta) = metadata.get(&monitor_id) {
                let mut failure_counters = self.failure_counters.lock().unwrap();
                let failure_key =
                    format!("{}:{}:{}", monitor_id, error_type, status_code.unwrap_or(0));

                let counter = failure_counters.entry(failure_key).or_insert_with(|| {
                    metrics::counter!(
                        "http_monitor_failures_total",
                        "monitor_id" => monitor_id.to_string(),
                        "monitor_name" => meta.name.clone(),
                        "monitor_url" => meta.url.clone(),
                        "interval_minutes" => meta.interval.to_string(),
                        "error_type" => error_type.to_string(),
                        "status_code" => status_code.map(|c| c.to_string()).unwrap_or_else(|| "none".to_string())
                    )
                });
                counter.increment(1);
            }
        }

        // Update status to down (0.0)
        if let Ok(gauges) = self.monitor_status_gauges.lock() {
            if let Some(gauge) = gauges.get(&monitor_id) {
                gauge.set(0.0);
            }
        }
    }
}

/// Initialize metrics system with descriptions for all metrics
pub fn init_metrics() {
    metrics::describe_histogram!(
        "http_monitor_response_time_seconds",
        Unit::Seconds,
        "HTTP response time in seconds"
    );

    metrics::describe_counter!(
        "http_monitor_requests_total",
        Unit::Count,
        "Total HTTP requests by monitor and status"
    );

    metrics::describe_counter!(
        "http_monitor_failures_total",
        Unit::Count,
        "Total HTTP failures by monitor, error type, and status code"
    );

    metrics::describe_gauge!(
        "http_monitor_up",
        Unit::Count,
        "Whether the monitor is currently up (1) or down (0)"
    );

    metrics::describe_gauge!(
        "http_monitor_last_success_timestamp",
        Unit::Seconds,
        "Unix timestamp of last successful check"
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_metrics_registry_creation() {
        let registry = MetricsRegistry::new();
        let monitor_id = Uuid::new_v4();

        let metadata = MonitorMetadata {
            name: "Test Monitor".to_string(),
            url: "https://example.com".to_string(),
            interval: 60,
        };

        registry.register_monitor(monitor_id, metadata);

        // Registry created successfully
    }

    #[test]
    fn test_success_recording() {
        let registry = MetricsRegistry::new();
        let monitor_id = Uuid::new_v4();

        let metadata = MonitorMetadata {
            name: "Success Test".to_string(),
            url: "https://success.com".to_string(),
            interval: 30,
        };

        registry.register_monitor(monitor_id, metadata);
        registry.record_success(monitor_id, 150);

        // Test passes if no panics occur
        assert!(true);
    }

    #[test]
    fn test_failure_recording() {
        let registry = MetricsRegistry::new();
        let monitor_id = Uuid::new_v4();

        let metadata = MonitorMetadata {
            name: "Failure Test".to_string(),
            url: "https://failure.com".to_string(),
            interval: 60,
        };

        registry.register_monitor(monitor_id, metadata);
        registry.record_failure(monitor_id, 5000, "timeout", None);
        registry.record_failure(monitor_id, 200, "http_error", Some(500));

        // Test passes if no panics occur
        assert!(true);
    }
}
