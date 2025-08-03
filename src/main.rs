//mod metrics;
//mod prometheus_client;
//mod settings;
//mod worker;

use axum::{
    extract::{Path, Request, State},
    http::{StatusCode},
    middleware::{self, Next},
    response::{Html, IntoResponse},
    routing::get,
    Json, Router,
};
use clap::{arg, Command};
use sammy_monitor::metrics::init_metrics;
use metrics_exporter_prometheus::{Matcher, PrometheusBuilder, PrometheusHandle};
use sammy_monitor::prometheus_client::PrometheusClient;
use sammy_monitor::monitor_detail::MonitorDetailContext;
use sammy_monitor::settings::{MonitorConfig, Settings};
use std::path::PathBuf;
use tera::Tera;
use tower_http::trace::TraceLayer;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};
use sammy_monitor::worker::Worker;

// Add these constants and types - you'll need to define them based on your app
const APP_NAME: &str = "sammy_monitor";
const APP_VERSION: &str = "0.1.0";

#[derive(Clone)]
struct AppState {
    monitors: Vec<MonitorConfig>,
    templates: Tera,
    prometheus: PrometheusClient,
}

#[derive(serde::Serialize)]
struct ApiError {
    message: String,
}

fn setup_metrics_recorder() -> PrometheusHandle {
    let handle = PrometheusBuilder::new()
        .add_global_label("app", "sammy_monitor")
        .set_buckets_for_metric(
            Matcher::Full("http_monitor_response_time_seconds".to_string()),
            &[0.05, 0.1, 0.2, 0.5, 1.0, 2.0, 5.0, 10.0, 20.0, 30.0],
        )
        .expect("Failed to set histogram buckets")
        .install_recorder()
        .expect("Failed to install Prometheus recorder");

    init_metrics();
    handle
}

async fn track_metrics(req: Request, next: Next) -> impl IntoResponse {
    // HTTP monitoring metrics are now handled by the worker thread
    // This middleware could be used for API endpoint metrics if needed
    next.run(req).await
}

fn metrics_app() -> Router {
    let handle = setup_metrics_recorder();
    Router::new().route("/metrics", get(move || async move { handle.render() }))
}

fn main_app(app_state: AppState) -> Router {
    Router::new()
        .route("/", get(index))
        .route("/monitor/:monitor_id", get(monitor_detail))
        .route("/health", get(health))
        .with_state(app_state)
        .route_layer(middleware::from_fn(track_metrics))
        .layer(TraceLayer::new_for_http())
        .fallback(unhandled)
}

async fn start_metrics_server() {
    let app = metrics_app();

    let listener = tokio::net::TcpListener::bind("0.0.0.0:3001").await.unwrap();
    tracing::debug!("listening on {}", listener.local_addr().unwrap());
    axum::serve(listener, app).await.unwrap();
}

async fn start_main_server(app_state: AppState) {
    let app = main_app(app_state);

    let listener = tokio::net::TcpListener::bind("0.0.0.0:3000").await.unwrap();
    tracing::debug!("listening on {}", listener.local_addr().unwrap());
    axum::serve(listener, app).await.unwrap();
}

async fn start_worker(settings: Settings) {
    let mut worker = Worker::new(settings);
    worker.start().await;
}

async fn health() -> impl IntoResponse {
    (StatusCode::OK, "OK")
}

async fn index(State(_state): State<AppState>) -> impl IntoResponse {
    Html("hi")
}

async fn monitor_detail(
    Path(monitor_id): Path<String>,
    State(state): State<AppState>,
) -> impl IntoResponse {
    let monitor_detail_context = MonitorDetailContext::default();
        
    match monitor_detail_context.fetch(&monitor_id, &state.prometheus).await {
        Ok(context) => {
            // Debug: try to serialize context to see if there are issues
            match serde_json::to_string_pretty(&context) {
                Ok(json_debug) => {
                    println!("Context debug: {}", json_debug);
                }
                Err(e) => {
                    println!("Context serialization error: {}", e);
                }
            }

            match state.templates.render(
                "monitor_detail.html",
                &tera::Context::from_serialize(&context).unwrap(),
            ) {
                Ok(html) => Html(html),
                Err(e) => Html(format!(
                    "<h1>Template error</h1><p>Detailed error: {}</p>",
                    e
                )),
            }
        }
        Err(e) => Html(format!("<h1>Monitor not found</h1><p>{}</p>", e)),
    }
}

async fn _calculate_uptime(
    monitor_id: &str,
    period: &str,
    prometheus: &PrometheusClient,
) -> Result<f64, Box<dyn std::error::Error>> {
    let success_query = format!(
        "increase(http_monitor_requests_total{{monitor_id=\"{}\",status=\"success\"}}[{}])",
        monitor_id, period
    );
    let total_query = format!(
        "increase(http_monitor_requests_total{{monitor_id=\"{}\"}}[{}])",
        monitor_id, period
    );

    let success_response = prometheus.query(&success_query).await?;
    let total_response = prometheus.query(&total_query).await?;

    let success_count = if let Some(results) = success_response["data"]["result"].as_array() {
        if !results.is_empty() {
            results[0]["value"][1]
                .as_str()
                .unwrap_or("0")
                .parse::<f64>()
                .unwrap_or(0.0)
        } else {
            0.0
        }
    } else {
        0.0
    };

    let total_count: f64 = if let Some(results) = total_response["data"]["result"].as_array() {
        results
            .iter()
            .map(|r| {
                r["value"][1]
                    .as_str()
                    .unwrap_or("0")
                    .parse::<f64>()
                    .unwrap_or(0.0)
            })
            .sum()
    } else {
        0.0
    };

    if total_count > 0.0 {
        Ok((success_count / total_count) * 100.0)
    } else {
        Ok(0.0)
    }
}

async fn _calculate_avg_response(
    monitor_id: &str,
    period: &str,
    prometheus: &PrometheusClient,
) -> Result<f64, Box<dyn std::error::Error>> {
    let query = format!("avg_over_time((rate(http_monitor_response_time_seconds_sum{{monitor_id=\"{}\"}}[5m]) / rate(http_monitor_response_time_seconds_count{{monitor_id=\"{}\"}}[5m]))[{}:1h])", monitor_id, monitor_id, period);
    let response = prometheus.query(&query).await?;

    if let Some(results) = response["data"]["result"].as_array() {
        if !results.is_empty() {
            let avg_time = results[0]["value"][1]
                .as_str()
                .unwrap_or("0")
                .parse::<f64>()
                .unwrap_or(0.0);
            return Ok(avg_time * 1000.0); // Convert to ms
        }
    }

    Ok(0.0)
}


/// Fallback for unmatched routes.
async fn unhandled() -> impl IntoResponse {
    (
        StatusCode::NOT_FOUND,
        Json(ApiError {
            message: "Resource not found".to_string(),
        }),
    )
}

fn cli() -> clap::Command {
    Command::new(APP_NAME)
        .version(APP_VERSION)
        .author("Greg Hewett <glh@strand3.com>")
        .about("The Sammy Monitoring Server")
        .arg(
            arg!(settings: [PATH])
                .long("settings")
                .default_value("./settings.toml")
                .help("Path to the settings file"),
        )
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let matches = cli().get_matches();

    let settings_path = matches
        .get_one::<String>("settings")
        .expect("settings is required");

    let settings =
        Settings::load(&PathBuf::from(settings_path.as_str())).expect("failed to load settings");

    let tera = Tera::new("templates/**/*").expect("Failed to initialize Tera");

    let prometheus = PrometheusClient {
        url: settings.prometheus_url.clone().unwrap(),
    };

    let state = AppState {
        monitors: settings.monitors.clone(),
        templates: tera.clone(),
        prometheus: prometheus.clone(),
    };

    tracing_subscriber::registry()
        .with(
            tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| {
                format!("{}=debug,tower_http=debug", env!("CARGO_CRATE_NAME")).into()
            }),
        )
        .with(tracing_subscriber::fmt::layer())
        .init();

    let (_main_server, _metrics_server, _worker) = tokio::join!(
        start_main_server(state),
        start_metrics_server(),
        start_worker(settings)
    );

    Ok(())
}
