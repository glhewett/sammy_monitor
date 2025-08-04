use axum::{routing::get, Router};
use clap::{Command, arg};
use metrics_exporter_prometheus::{Matcher, PrometheusBuilder, PrometheusHandle};
use sammy_monitor::metrics::init_metrics;
use sammy_monitor::settings::Settings;
use sammy_monitor::worker::Worker;
use std::path::PathBuf;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

const APP_NAME: &str = "sammy_monitor";
const APP_VERSION: &str = "0.1.0";

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

fn create_app() -> Router {
    let handle = setup_metrics_recorder();
    Router::new().route("/metrics", get(move || async move { handle.render() }))
}

async fn start_server() {
    let app = create_app();

    let listener = tokio::net::TcpListener::bind("0.0.0.0:3000").await.unwrap();
    tracing::info!("Metrics server listening on {}", listener.local_addr().unwrap());
    axum::serve(listener, app).await.unwrap();
}

async fn start_worker(settings: Settings) {
    let mut worker = Worker::new(settings);
    worker.start().await;
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

    tracing_subscriber::registry()
        .with(
            tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| {
                format!("{}=debug,tower_http=debug", env!("CARGO_CRATE_NAME")).into()
            }),
        )
        .with(tracing_subscriber::fmt::layer())
        .init();

    let (_server, _worker) = tokio::join!(
        start_server(),
        start_worker(settings)
    );

    Ok(())
}