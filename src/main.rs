use std::path::PathBuf;
use std::sync::Arc;
use std::time::Instant;

use axum::{
    Json, Router,
    extract::{MatchedPath, Request, State},
    http::StatusCode,
    middleware::{self, Next},
    response::{Html, IntoResponse},
    routing::get,
};
use clap::{Command, arg};
use metrics_exporter_prometheus::{Matcher, PrometheusBuilder, PrometheusHandle};
use serde::Deserialize;
use std::fs::read_to_string;
use std::future::ready;
use std::io::{Error, ErrorKind};
use tera::Tera;
use tokio;
use tower_http::trace::TraceLayer;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

// Add these constants and types - you'll need to define them based on your app
const APP_NAME: &str = "sammy_monitor";
const APP_VERSION: &str = "0.1.0";

#[derive(Deserialize, Debug, Clone)]
pub struct MonitorConfig {
    pub name: String,
    pub url: String,
    pub interval: u64, // in seconds
}

#[derive(Deserialize, Debug, Clone)]
pub struct Settings {
    pub monitors: Vec<MonitorConfig>,
}

impl Settings {
    pub fn load(path: &PathBuf) -> Result<Settings, Error> {
        if !path.exists() {
            return Err(Error::new(
                ErrorKind::NotFound,
                "Configuration was not found.",
            ));
        }

        let config_file_contents = match read_to_string(path) {
            Ok(contents) => contents,
            Err(e) => {
                return Err(Error::new(
                    ErrorKind::NotFound,
                    format!("Unable to read configuration. {}", e),
                ));
            }
        };

        let settings: Settings = match toml::from_str(config_file_contents.as_str()) {
            Ok(token) => token,
            Err(e) => {
                return Err(Error::new(
                    ErrorKind::NotFound,
                    format!("Unable to parse configuration. {}", e),
                ));
            }
        };

        Ok(settings)
    }
}

#[derive(Clone)]
struct AppState {
    settings: Arc<Settings>,
    templates: Arc<Tera>,
}

#[derive(serde::Serialize)]
struct IndexContext {
    title: String,
    subtitle: String,
    monitored_urls: Vec<String>,
}

#[derive(serde::Serialize)]
struct ApiError {
    message: String,
}

fn setup_metrics_recorder() -> PrometheusHandle {
    const EXPONENTIAL_SECONDS: &[f64] = &[
        0.005, 0.01, 0.025, 0.05, 0.1, 0.25, 0.5, 1.0, 2.5, 5.0, 10.0,
    ];

    PrometheusBuilder::new()
        .set_buckets_for_metric(
            Matcher::Full("http_requests_duration_seconds".to_string()),
            EXPONENTIAL_SECONDS,
        )
        .unwrap()
        .install_recorder()
        .unwrap()
}

async fn track_metrics(req: Request, next: Next) -> impl IntoResponse {
    let start = Instant::now();
    let path = if let Some(matched_path) = req.extensions().get::<MatchedPath>() {
        matched_path.as_str().to_owned()
    } else {
        req.uri().path().to_owned()
    };
    let method = req.method().clone();

    let response = next.run(req).await;

    let latency = start.elapsed().as_secs_f64();
    let status = response.status().as_u16().to_string();

    let labels = [
        ("method", method.to_string()),
        ("path", path),
        ("status", status),
    ];

    metrics::counter!("http_requests_total", &labels).increment(1);
    metrics::histogram!("http_requests_duration_seconds", &labels).record(latency);

    response
}

fn metrics_app() -> Router {
    let recorder_handle = setup_metrics_recorder();
    Router::new().route("/metrics", get(move || ready(recorder_handle.render())))
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

fn init_templates() -> Tera {
    // Load all .html or .tera templates in templates/ directory
    Tera::new("templates/*.html").expect("Failed to initialize Tera")
}

async fn index(State(state): State<AppState>) -> impl IntoResponse {
    let mut context = IndexContext {
        title: "Sammy's HTTP Monitor".to_string(),
        subtitle: "Welcome to Sammy's HTTP Monitor".to_string(),
        monitored_urls: vec![],
    };

    for site in &state.settings.monitors {
        context.monitored_urls.push(site.url.clone());
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
        .about("Start the TacoCat API server")
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

    let (_main_server, _metrics_server) =
        tokio::join!(start_main_server(state), start_metrics_server());

    Ok(())
}
