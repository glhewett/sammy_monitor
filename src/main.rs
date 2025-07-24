mod metrics;
mod settings;
mod worker;

use axum::{
    Json, Router,
    extract::{Request, State},
    http::StatusCode,
    middleware::{self, Next},
    response::{Html, IntoResponse},
    routing::get,
};
use clap::{Command, arg};
use metrics_exporter_prometheus::{PrometheusBuilder, PrometheusHandle};
use std::path::PathBuf;
use std::sync::Arc;
use tera::Tera;
use tower_http::trace::TraceLayer;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

use metrics::init_metrics;
use settings::Settings;
use worker::Worker;

// Add these constants and types - you'll need to define them based on your app
const APP_NAME: &str = "sammy_monitor";
const APP_VERSION: &str = "0.1.0";

#[derive(Clone)]
struct AppState {
    settings: Arc<Settings>,
    templates: Arc<Tera>,
}

#[derive(serde::Serialize)]
struct IndexContext {
    title: String,
    subtitle: String,
    monitor_ids: Vec<String>,
}

#[derive(serde::Serialize)]
struct ApiError {
    message: String,
}

fn setup_metrics_recorder() -> PrometheusHandle {
    let handle = PrometheusBuilder::new()
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

fn init_templates() -> Tera {
    // Load all .html or .tera templates in templates/ directory
    Tera::new("templates/*.html").expect("Failed to initialize Tera")
}

async fn index(State(state): State<AppState>) -> impl IntoResponse {
    let mut context = IndexContext {
        title: "Sammy's HTTP Monitor".to_string(),
        subtitle: "Welcome to Sammy's HTTP Monitor".to_string(),
        monitor_ids: vec![],
    };

    for site in &state.settings.monitors {
        context.monitor_ids.push(site.id.to_string());
    }

    let rendered = state.templates.render(
        "index.html",
        &tera::Context::from_serialize(&context).unwrap(),
    );

    match rendered {
        Ok(html) => Html(html).into_response(),
        Err(err) => {
            eprintln!("Template error: {:?}", err);
            (
                axum::http::StatusCode::INTERNAL_SERVER_ERROR,
                Html("Internal Server Error".to_string()),
            )
                .into_response()
        }
    }
}

async fn health() -> impl IntoResponse {
    (StatusCode::OK, "OK")
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

    let state = AppState {
        settings: Arc::new(settings.clone()),
        templates: Arc::new(init_templates()),
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
