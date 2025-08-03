use log::info;
use serde_json::Value as JsonValue;
use std::time::Instant;

#[derive(Debug, Clone)]
pub struct PrometheusClient {
    pub url: String,
}

impl PrometheusClient {
    pub async fn query(&self, query: &str) -> Result<serde_json::Value, reqwest::Error> {
        let start_time = Instant::now();
        info!("Query: {query}");

        let url = format!(
            "{}/api/v1/query?query={}",
            self.url,
            urlencoding::encode(query)
        );
        let response = reqwest::get(&url).await?;
        let json: JsonValue = response.json().await?;
        info!("Response: {json}");
        info!(
            "Request took {} milliseconds",
            start_time.elapsed().as_millis()
        );
        Ok(json)
    }
}
