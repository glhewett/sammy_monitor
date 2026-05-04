use sammy_monitor::db::Db;
use sammy_monitor::settings::{Settings, SmtpConfig};
use sammy_monitor::worker::Worker;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

use clap::{arg, Command};

const APP_NAME: &str = "sammy_monitor";
const APP_VERSION: &str = "0.1.0";

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

    // Load .env then .env.local (both are optional; .env.local overrides .env)
    let _ = dotenvy::from_filename(".env");
    let _ = dotenvy::from_filename(".env.local");

    let smtp_config = SmtpConfig::from_env();
    if smtp_config.is_none() {
        tracing::warn!("SMTP not configured — email alerts disabled");
    }

    let db = Db::open(&settings.db_path)?;
    let db = Arc::new(Mutex::new(db));

    let mut worker = Worker::new(settings, db, smtp_config);
    worker.start().await;

    Ok(())
}
