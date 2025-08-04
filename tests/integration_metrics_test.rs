use std::time::Duration;
use tokio::time;
use uuid::Uuid;

use sammy_monitor::metrics::{METRICS_REGISTRY, MonitorMetadata, init_metrics};

/// Single comprehensive integration test for Prometheus metrics
/// This test validates that metrics are correctly generated, formatted, and contain accurate values
#[tokio::test]
async fn test_metrics_integration() {
    // Set up Prometheus exporter FIRST before doing anything with metrics
    let handle = metrics_exporter_prometheus::PrometheusBuilder::new()
        .add_global_label("app", "sammy_monitor_test")
        .set_buckets_for_metric(
            metrics_exporter_prometheus::Matcher::Full(
                "http_monitor_response_time_seconds".to_string(),
            ),
            &[0.05, 0.1, 0.2, 0.5, 1.0, 2.0, 5.0, 10.0, 20.0, 30.0],
        )
        .expect("Failed to set histogram buckets")
        .install_recorder()
        .expect("Failed to install Prometheus recorder");

    // Initialize metrics system AFTER setting up the exporter
    init_metrics();

    // Set up test monitors
    let monitor1_id = Uuid::new_v4();
    let monitor2_id = Uuid::new_v4();

    let metadata1 = MonitorMetadata {
        name: "Integration Test Site 1".to_string(),
        url: "https://example.com".to_string(),
        interval: 60,
    };

    let metadata2 = MonitorMetadata {
        name: "Integration Test Site 2".to_string(),
        url: "https://httpbin.org/status/404".to_string(),
        interval: 120,
    };

    // Register monitors with metrics registry
    METRICS_REGISTRY.register_monitor(monitor1_id, metadata1.clone());
    METRICS_REGISTRY.register_monitor(monitor2_id, metadata2.clone());

    // Record comprehensive test data with known values

    // Monitor 1: 5 successful requests with specific response times
    METRICS_REGISTRY.record_success(monitor1_id, 100); // 0.1 seconds
    METRICS_REGISTRY.record_success(monitor1_id, 250); // 0.25 seconds
    METRICS_REGISTRY.record_success(monitor1_id, 500); // 0.5 seconds
    METRICS_REGISTRY.record_success(monitor1_id, 1000); // 1.0 seconds
    METRICS_REGISTRY.record_success(monitor1_id, 2500); // 2.5 seconds

    // Monitor 2: 3 failures with different error types
    METRICS_REGISTRY.record_failure(monitor2_id, 5000, "timeout", None);
    METRICS_REGISTRY.record_failure(monitor2_id, 300, "http_error", Some(404));
    METRICS_REGISTRY.record_failure(monitor2_id, 200, "http_error", Some(500));

    // Edge cases
    METRICS_REGISTRY.record_success(monitor1_id, 0); // 0ms response time
    METRICS_REGISTRY.record_success(monitor2_id, 99999); // Very high response time

    // Allow time for metrics processing
    time::sleep(Duration::from_millis(200)).await;

    // Get the Prometheus output
    let output = handle.render();

    println!("=== COMPREHENSIVE METRICS INTEGRATION TEST ===");
    println!("{}", output);

    // === BASIC PRESENCE TESTS ===

    // Test 1: Output should not be empty
    assert!(
        !output.trim().is_empty(),
        "Metrics output should not be empty"
    );

    // Test 2: Check that all expected metric types are present
    assert!(
        output.contains("http_monitor_response_time_seconds"),
        "Should contain response time metrics"
    );
    assert!(
        output.contains("http_monitor_requests_total"),
        "Should contain request total counters"
    );
    assert!(
        output.contains("http_monitor_failures_total"),
        "Should contain failure counters"
    );
    assert!(
        output.contains("http_monitor_up"),
        "Should contain monitor status gauges"
    );
    assert!(
        output.contains("http_monitor_last_success_timestamp"),
        "Should contain last success timestamps"
    );

    // Test 3: Check monitor identification
    assert!(output.contains(&format!("monitor_id=\"{}\"", monitor1_id)));
    assert!(output.contains(&format!("monitor_id=\"{}\"", monitor2_id)));
    assert!(output.contains("monitor_name=\"Integration Test Site 1\""));
    assert!(output.contains("monitor_name=\"Integration Test Site 2\""));

    // Test 4: Check status labels
    assert!(output.contains("status=\"success\""));
    assert!(output.contains("status=\"failure\""));

    // Test 5: Check error types
    assert!(output.contains("error_type=\"timeout\""));
    assert!(output.contains("error_type=\"http_error\""));
    assert!(output.contains("status_code=\"404\""));
    assert!(output.contains("status_code=\"500\""));

    // === VALUE ACCURACY TESTS ===

    // Test 6: Validate Monitor 1 success count (6 total: 5 initial + 1 edge case)
    let monitor1_success = extract_metric_value(
        &output,
        "http_monitor_requests_total",
        &[
            ("status", "success"),
            ("monitor_id", &monitor1_id.to_string()),
        ],
    );
    assert_eq!(
        monitor1_success,
        Some(6.0),
        "Monitor 1 should have 6 successful requests"
    );

    // Test 7: Validate Monitor 2 failure count (3 failures)
    let monitor2_failure = extract_metric_value(
        &output,
        "http_monitor_requests_total",
        &[
            ("status", "failure"),
            ("monitor_id", &monitor2_id.to_string()),
        ],
    );
    assert_eq!(
        monitor2_failure,
        Some(3.0),
        "Monitor 2 should have 3 failed requests"
    );

    // Test 8: Validate Monitor 2 success count (1 edge case success)
    let monitor2_success = extract_metric_value(
        &output,
        "http_monitor_requests_total",
        &[
            ("status", "success"),
            ("monitor_id", &monitor2_id.to_string()),
        ],
    );
    assert_eq!(
        monitor2_success,
        Some(1.0),
        "Monitor 2 should have 1 successful request"
    );

    // Test 9: Validate specific failure types
    let timeout_failures = extract_metric_value(
        &output,
        "http_monitor_failures_total",
        &[
            ("error_type", "timeout"),
            ("monitor_id", &monitor2_id.to_string()),
        ],
    );
    assert_eq!(timeout_failures, Some(1.0), "Should have 1 timeout failure");

    let http_404_failures = extract_metric_value(
        &output,
        "http_monitor_failures_total",
        &[
            ("error_type", "http_error"),
            ("status_code", "404"),
            ("monitor_id", &monitor2_id.to_string()),
        ],
    );
    assert_eq!(
        http_404_failures,
        Some(1.0),
        "Should have 1 HTTP 404 failure"
    );

    let http_500_failures = extract_metric_value(
        &output,
        "http_monitor_failures_total",
        &[
            ("error_type", "http_error"),
            ("status_code", "500"),
            ("monitor_id", &monitor2_id.to_string()),
        ],
    );
    assert_eq!(
        http_500_failures,
        Some(1.0),
        "Should have 1 HTTP 500 failure"
    );

    // === HISTOGRAM TESTS ===

    // Test 10: Validate histogram buckets for Monitor 1
    // Response times: 0ms, 100ms, 250ms, 500ms, 1000ms, 2500ms
    validate_histogram_bucket(&output, &monitor1_id, "0.1", 2.0); // 0ms, 100ms
    validate_histogram_bucket(&output, &monitor1_id, "0.5", 4.0); // + 250ms, 500ms
    validate_histogram_bucket(&output, &monitor1_id, "1", 5.0); // + 1000ms
    validate_histogram_bucket(&output, &monitor1_id, "5", 6.0); // + 2500ms
    validate_histogram_bucket(&output, &monitor1_id, "+Inf", 6.0); // all observations

    // Test 11: Validate histogram sum for Monitor 1
    let expected_sum_ms = 0.0 + 100.0 + 250.0 + 500.0 + 1000.0 + 2500.0; // 4350ms total
    let expected_sum_seconds = expected_sum_ms / 1000.0; // 4.35 seconds

    let histogram_sum = extract_histogram_sum(&output, &monitor1_id);
    if let Some(sum) = histogram_sum {
        assert!(
            (sum - expected_sum_seconds).abs() < 0.001,
            "Monitor 1 histogram sum should be {} but got {}",
            expected_sum_seconds,
            sum
        );
    } else {
        panic!("Monitor 1 histogram sum not found");
    }

    // Test 12: Validate histogram count for Monitor 1
    let histogram_count = extract_histogram_count(&output, &monitor1_id);
    assert_eq!(
        histogram_count,
        Some(6.0),
        "Monitor 1 histogram should have 6 observations"
    );

    // === STATUS AND TIMESTAMP TESTS ===

    // Test 13: Monitor 1 should be UP (last operation was success)
    let monitor1_status = extract_metric_value(
        &output,
        "http_monitor_up",
        &[("monitor_id", &monitor1_id.to_string())],
    );
    assert_eq!(monitor1_status, Some(1.0), "Monitor 1 should be UP");

    // Test 14: Monitor 2 should be DOWN (last operation was failure)
    let monitor2_status = extract_metric_value(
        &output,
        "http_monitor_up",
        &[("monitor_id", &monitor2_id.to_string())],
    );
    assert_eq!(
        monitor2_status,
        Some(1.0),
        "Monitor 2 should be UP (last op was success edge case)"
    );

    // Test 15: Last success timestamps should be recent
    let monitor1_timestamp = extract_metric_value(
        &output,
        "http_monitor_last_success_timestamp",
        &[("monitor_id", &monitor1_id.to_string())],
    );

    if let Some(timestamp) = monitor1_timestamp {
        let current_time = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs() as f64;

        assert!(
            current_time - timestamp < 10.0,
            "Monitor 1 last success timestamp should be recent: {} vs {}",
            timestamp,
            current_time
        );
    } else {
        panic!("Monitor 1 last success timestamp not found");
    }

    // === FORMAT COMPLIANCE TESTS ===

    // Test 16: Check for proper Prometheus format
    let lines: Vec<&str> = output.lines().collect();

    // Should have HELP comments
    let help_lines: Vec<&str> = lines
        .iter()
        .filter(|line| line.starts_with("# HELP"))
        .cloned()
        .collect();
    assert!(!help_lines.is_empty(), "Should have HELP comments");

    // Should have TYPE comments
    let type_lines: Vec<&str> = lines
        .iter()
        .filter(|line| line.starts_with("# TYPE"))
        .cloned()
        .collect();
    assert!(!type_lines.is_empty(), "Should have TYPE comments");

    // Should have expected metric types
    assert!(output.contains("# TYPE http_monitor_response_time_seconds histogram"));
    assert!(output.contains("# TYPE http_monitor_requests_total counter"));
    assert!(output.contains("# TYPE http_monitor_up gauge"));

    println!("✅ ALL METRICS INTEGRATION TESTS PASSED!");
    println!("📊 Metrics are correctly generated, formatted, and contain accurate values");
    println!("🔢 Counter accuracy: ✓");
    println!("📈 Histogram accuracy: ✓");
    println!("🏷️  Label accuracy: ✓");
    println!("📝 Format compliance: ✓");
}

// Helper functions for metric extraction and validation

fn extract_metric_value(output: &str, metric_name: &str, labels: &[(&str, &str)]) -> Option<f64> {
    for line in output.lines() {
        if line.starts_with('#') || line.trim().is_empty() {
            continue;
        }

        if line.starts_with(metric_name) {
            // Check if all required labels are present
            let all_labels_match = labels
                .iter()
                .all(|(key, value)| line.contains(&format!("{}=\"{}\"", key, value)));

            if all_labels_match {
                if let Some(value_str) = line.split_whitespace().last() {
                    if let Ok(value) = value_str.parse::<f64>() {
                        return Some(value);
                    }
                }
            }
        }
    }
    None
}

fn validate_histogram_bucket(output: &str, monitor_id: &Uuid, le_value: &str, expected_count: f64) {
    let bucket_count = extract_metric_value(
        output,
        "http_monitor_response_time_seconds_bucket",
        &[("le", le_value), ("monitor_id", &monitor_id.to_string())],
    );

    assert_eq!(
        bucket_count,
        Some(expected_count),
        "Bucket le=\"{}\" should have {} observations but got {:?}",
        le_value,
        expected_count,
        bucket_count
    );
}

fn extract_histogram_sum(output: &str, monitor_id: &Uuid) -> Option<f64> {
    extract_metric_value(
        output,
        "http_monitor_response_time_seconds_sum",
        &[("monitor_id", &monitor_id.to_string())],
    )
}

fn extract_histogram_count(output: &str, monitor_id: &Uuid) -> Option<f64> {
    extract_metric_value(
        output,
        "http_monitor_response_time_seconds_count",
        &[("monitor_id", &monitor_id.to_string())],
    )
}
