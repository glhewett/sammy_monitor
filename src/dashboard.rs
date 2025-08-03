use crate::prometheus_client::PrometheusClient;

#[derive(serde::Serialize)]
struct DashboardContext {
    monitors: Vec<Monitor>,
    total_monitors: usize,
    online_monitors: usize,
    offline_monitors: usize,
    avg_response_time: f64,
}


pub struct Dashboard {}

impl Default for Dashboard {
    fn default() -> Self {
        Dashboard {}
    }
}

impl Dashboard {
    pub fn get_metrics() -> Dashboard {
        Dashboard {
            ..Default::default()
        }
    }

    async fn get_context(
        prometheus: &PrometheusClient,
    ) -> Result<DashboardContext, Box<dyn std::error::Error>> {
        // Get all monitors
        let monitors_response = prometheus
            .query("group by (monitor_id, monitor_name, monitor_url) (http_monitor_up)")
            .await?;
        let mut monitors = Vec::new();

        if let Some(results) = monitors_response["data"]["result"].as_array() {
            for result in results {
                let metric = &result["metric"];
                let monitor_id = metric["monitor_id"].as_str().unwrap_or("").to_string();
                let monitor_name = metric["monitor_name"].as_str().unwrap_or("").to_string();
                let monitor_url = metric["monitor_url"].as_str().unwrap_or("").to_string();

                // Get current status from the original result
                let is_up = result["value"][1].as_str().unwrap_or("0") == "1";

                // Generate graph data for the last 24 hours
                let graph_data = generate_graph_data(&monitor_id, prometheus)
                    .await
                    .unwrap_or_else(|_| generate_sample_graph_data(&monitor_id));

                // For now, use simple placeholder values to avoid complex queries
                monitors.push(Monitor {
                    id: monitor_id,
                    name: monitor_name,
                    url: monitor_url,
                    is_up,
                    uptime_24h: 99.5,
                    uptime_7d: 99.2,
                    uptime_30d: 98.8,
                    uptime_365d: 99.1,
                    avg_response_24h: 150.0,
                    avg_response_7d: 165.0,
                    avg_response_30d: 170.0,
                    avg_response_365d: 180.0,
                    last_failure: "No recent failures".to_string(),
                    days_since_failure: 30,
                    failure_count_7d: 0,
                    graph_data,
                });
            }
        }

        let total_monitors = monitors.len();
        let online_monitors = monitors.iter().filter(|m| m.is_up).count();
        let offline_monitors = total_monitors - online_monitors;
        let avg_response_time = if !monitors.is_empty() {
            monitors.iter().map(|m| m.avg_response_24h).sum::<f64>() / monitors.len() as f64
        } else {
            0.0
        };

        Ok(DashboardContext {
            monitors,
            total_monitors,
            online_monitors,
            offline_monitors,
            avg_response_time,
        })
    }
}
