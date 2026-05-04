use serde::Deserialize;
use std::env;
use std::fs;
use std::io::{Error, ErrorKind};
use std::path::PathBuf;
use std::str::FromStr;
use uuid::Uuid;

#[derive(Deserialize, Debug, Clone)]
pub struct MonitorConfig {
    pub id: Uuid,
    pub name: String,
    pub url: String,
    pub interval: u64,
    pub enabled: bool,
}

/// SMTP configuration loaded from environment variables.
///
/// Expected env vars:
///   SMTP_HOST, SMTP_PORT, SMTP_USER, SMTP_PASS, SMTP_FROM, SMTP_TO
///
/// Optional:
///   SMTP_TLS  (true/false, defaults to true)
///   SMTP_TO   (comma-separated for multiple recipients)
#[derive(Debug, Clone)]
pub struct SmtpConfig {
    pub smtp_host: String,
    pub smtp_port: u16,
    pub smtp_username: String,
    pub smtp_password: String,
    pub smtp_use_tls: bool,
    pub from_address: String,
    pub to_addresses: Vec<String>,
}

impl SmtpConfig {
    /// Load SMTP config from environment variables. Returns `None` if any
    /// required variable is missing, logging a warning for each gap.
    pub fn from_env() -> Option<Self> {
        let mut ok = true;

        macro_rules! require {
            ($key:expr) => {
                match env::var($key) {
                    Ok(v) => v,
                    Err(_) => {
                        log::warn!("Missing required env var: {}", $key);
                        ok = false;
                        String::new()
                    }
                }
            };
        }

        let smtp_host = require!("SMTP_HOST");
        let smtp_port_str = require!("SMTP_PORT");
        let smtp_username = require!("SMTP_USER");
        let smtp_password = require!("SMTP_PASS");
        let from_address = require!("SMTP_FROM");
        let smtp_to = require!("SMTP_TO");

        if !ok {
            return None;
        }

        let smtp_port: u16 = match smtp_port_str.parse() {
            Ok(p) => p,
            Err(_) => {
                log::warn!("SMTP_PORT is not a valid port number: {smtp_port_str}");
                return None;
            }
        };

        let smtp_use_tls = env::var("SMTP_TLS")
            .map(|v| v.to_lowercase() != "false")
            .unwrap_or(true);

        let to_addresses = smtp_to
            .split(',')
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .collect::<Vec<_>>();

        if to_addresses.is_empty() {
            log::warn!("SMTP_TO is empty — no recipients configured");
            return None;
        }

        Some(Self {
            smtp_host,
            smtp_port,
            smtp_username,
            smtp_password,
            smtp_use_tls,
            from_address,
            to_addresses,
        })
    }
}

fn default_db_path() -> String {
    "./sammy_monitor.db".to_string()
}

fn default_down_alert_threshold() -> u32 {
    5
}

#[derive(Deserialize, Debug, Clone)]
pub struct Settings {
    pub monitors: Vec<MonitorConfig>,
    #[serde(default = "default_db_path")]
    pub db_path: String,
    #[serde(default = "default_down_alert_threshold")]
    pub down_alert_threshold: u32,
}

impl Settings {
    pub fn load(path: &PathBuf) -> Result<Settings, Error> {
        if !path.exists() {
            return Err(Error::new(
                ErrorKind::NotFound,
                format!("Settings file not found: {}", path.display()),
            ));
        }

        let config_file_contents = match fs::read_to_string(path) {
            Ok(content) => content,
            Err(e) => {
                return Err(Error::new(
                    ErrorKind::InvalidData,
                    format!("Failed to read settings file: {e}"),
                ));
            }
        };

        let settings: Settings = match toml::from_str(config_file_contents.as_str()) {
            Ok(token) => token,
            Err(e) => {
                return Err(Error::new(
                    ErrorKind::InvalidData,
                    format!("Failed to parse settings file: {e}"),
                ));
            }
        };

        Ok(settings)
    }

}

impl FromStr for Settings {
    type Err = Error;

    fn from_str(content: &str) -> Result<Settings, Error> {
        match toml::from_str(content) {
            Ok(settings) => Ok(settings),
            Err(e) => Err(Error::new(
                ErrorKind::InvalidData,
                format!("Failed to parse settings: {e}"),
            )),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::NamedTempFile;

    #[test]
    fn test_settings_from_str_valid() {
        let toml_content = r#"
[[monitors]]
id = "550e8400-e29b-41d4-a716-446655440001"
name = "Example Site"
url = "https://example.com"
interval = 60
enabled = true

[[monitors]]
id = "550e8400-e29b-41d4-a716-446655440002"
name = "Google"
url = "https://google.com"
interval = 30
enabled = false
"#;

        let settings: Settings = toml_content.parse().expect("Failed to parse valid TOML");

        assert_eq!(settings.monitors.len(), 2);

        assert_eq!(settings.monitors[0].name, "Example Site");
        assert_eq!(settings.monitors[0].url, "https://example.com");
        assert_eq!(settings.monitors[0].interval, 60);

        assert_eq!(settings.monitors[1].name, "Google");
        assert_eq!(settings.monitors[1].url, "https://google.com");
        assert_eq!(settings.monitors[1].interval, 30);
    }

    #[test]
    fn test_settings_from_str_invalid() {
        let invalid_toml = r#"
[[monitors]]
name = "Missing URL"
interval = 60
"#;

        let result: Result<Settings, _> = invalid_toml.parse();
        assert!(result.is_err());
    }

    #[test]
    fn test_settings_from_str_empty_monitors() {
        let toml_content = r#"
monitors = []
"#;

        let settings: Settings = toml_content
            .parse()
            .expect("Failed to parse empty monitors");
        assert_eq!(settings.monitors.len(), 0);
    }

    #[test]
    fn test_settings_load_file_exists() {
        let mut temp_file = NamedTempFile::new().expect("Failed to create temp file");
        let toml_content = r#"
[[monitors]]
id = "550e8400-e29b-41d4-a716-446655440003"
name = "Test Site"
url = "https://test.com"
interval = 45
enabled = true
"#;

        temp_file
            .write_all(toml_content.as_bytes())
            .expect("Failed to write to temp file");
        let temp_path = temp_file.path().to_path_buf();

        let settings = Settings::load(&temp_path).expect("Failed to load settings from file");

        assert_eq!(settings.monitors.len(), 1);
        assert_eq!(settings.monitors[0].name, "Test Site");
        assert_eq!(settings.monitors[0].url, "https://test.com");
        assert_eq!(settings.monitors[0].interval, 45);
    }

    #[test]
    fn test_settings_load_file_not_found() {
        let non_existent_path = PathBuf::from("/path/that/does/not/exist/settings.toml");
        let result = Settings::load(&non_existent_path);

        assert!(result.is_err());
        let error = result.unwrap_err();
        assert_eq!(error.kind(), ErrorKind::NotFound);
    }

    #[test]
    fn test_settings_load_invalid_file_content() {
        let mut temp_file = NamedTempFile::new().expect("Failed to create temp file");
        let invalid_content = "this is not valid TOML content";

        temp_file
            .write_all(invalid_content.as_bytes())
            .expect("Failed to write to temp file");
        let temp_path = temp_file.path().to_path_buf();

        let result = Settings::load(&temp_path);

        assert!(result.is_err());
        let error = result.unwrap_err();
        assert_eq!(error.kind(), ErrorKind::InvalidData);
    }

    #[test]
    fn test_monitor_config_fields() {
        let monitor = MonitorConfig {
            id: uuid::Uuid::parse_str("550e8400-e29b-41d4-a716-446655440004").unwrap(),
            name: "Test Monitor".to_string(),
            url: "https://example.org".to_string(),
            interval: 120,
            enabled: true,
        };

        assert_eq!(monitor.name, "Test Monitor");
        assert_eq!(monitor.url, "https://example.org");
        assert_eq!(monitor.interval, 120);
        assert!(monitor.enabled);
    }

    #[test]
    fn test_monitor_config_enabled_true() {
        let toml_content = r#"
[[monitors]]
id = "550e8400-e29b-41d4-a716-446655440005"
name = "Enabled Monitor"
url = "https://enabled.com"
interval = 60
enabled = true
"#;

        let settings: Settings = toml_content.parse().expect("Failed to parse TOML");
        assert_eq!(settings.monitors.len(), 1);
        assert!(settings.monitors[0].enabled);
        assert_eq!(settings.monitors[0].name, "Enabled Monitor");
    }

    #[test]
    fn test_monitor_config_enabled_false() {
        let toml_content = r#"
[[monitors]]
id = "550e8400-e29b-41d4-a716-446655440006"
name = "Disabled Monitor"
url = "https://disabled.com"
interval = 60
enabled = false
"#;

        let settings: Settings = toml_content.parse().expect("Failed to parse TOML");
        assert_eq!(settings.monitors.len(), 1);
        assert!(!settings.monitors[0].enabled);
        assert_eq!(settings.monitors[0].name, "Disabled Monitor");
    }

    #[test]
    fn test_monitor_config_mixed_enabled_states() {
        let toml_content = r#"
[[monitors]]
id = "550e8400-e29b-41d4-a716-446655440007"
name = "First Monitor"
url = "https://first.com"
interval = 60
enabled = true

[[monitors]]
id = "550e8400-e29b-41d4-a716-446655440008"
name = "Second Monitor"
url = "https://second.com"
interval = 30
enabled = false

[[monitors]]
id = "550e8400-e29b-41d4-a716-446655440009"
name = "Third Monitor"
url = "https://third.com"
interval = 45
enabled = true
"#;

        let settings: Settings = toml_content.parse().expect("Failed to parse TOML");
        assert_eq!(settings.monitors.len(), 3);

        assert!(settings.monitors[0].enabled);
        assert_eq!(settings.monitors[0].name, "First Monitor");

        assert!(!settings.monitors[1].enabled);
        assert_eq!(settings.monitors[1].name, "Second Monitor");

        assert!(settings.monitors[2].enabled);
        assert_eq!(settings.monitors[2].name, "Third Monitor");
    }

    #[test]
    fn test_monitor_config_missing_enabled_field() {
        let toml_content = r#"
[[monitors]]
id = "550e8400-e29b-41d4-a716-44665544000a"
name = "Missing Enabled Field"
url = "https://missing.com"
interval = 60
"#;

        let result: Result<Settings, _> = toml_content.parse();
        assert!(result.is_err(), "Should fail when enabled field is missing");
    }

    #[test]
    fn test_settings_get_enabled_monitors() {
        let toml_content = r#"
[[monitors]]
id = "550e8400-e29b-41d4-a716-44665544000b"
name = "Active Monitor 1"
url = "https://active1.com"
interval = 60
enabled = true

[[monitors]]
id = "550e8400-e29b-41d4-a716-44665544000c"
name = "Inactive Monitor"
url = "https://inactive.com"
interval = 30
enabled = false

[[monitors]]
id = "550e8400-e29b-41d4-a716-44665544000d"
name = "Active Monitor 2"
url = "https://active2.com"
interval = 45
enabled = true
"#;

        let settings: Settings = toml_content.parse().expect("Failed to parse TOML");
        let enabled_monitors: Vec<&MonitorConfig> = settings
            .monitors
            .iter()
            .filter(|monitor| monitor.enabled)
            .collect();

        assert_eq!(enabled_monitors.len(), 2);
        assert_eq!(enabled_monitors[0].name, "Active Monitor 1");
        assert_eq!(enabled_monitors[1].name, "Active Monitor 2");
    }

    #[test]
    fn test_monitor_config_enable_disable() {
        let mut monitor = MonitorConfig {
            id: uuid::Uuid::parse_str("550e8400-e29b-41d4-a716-44665544000e").unwrap(),
            name: "Toggle Monitor".to_string(),
            url: "https://toggle.com".to_string(),
            interval: 60,
            enabled: true,
        };

        assert!(monitor.enabled);

        // Disable the monitor
        monitor.enabled = false;
        assert!(!monitor.enabled);

        // Re-enable the monitor
        monitor.enabled = true;
        assert!(monitor.enabled);
    }

    #[test]
    fn test_db_path_default() {
        let settings: Settings = "monitors = []".parse().expect("Failed to parse TOML");
        assert_eq!(settings.db_path, "./sammy_monitor.db");
    }

    #[test]
    fn test_db_path_custom() {
        let settings: Settings = "db_path = \"/data/monitor.db\"\nmonitors = []"
            .parse()
            .expect("Failed to parse TOML");
        assert_eq!(settings.db_path, "/data/monitor.db");
    }

    #[test]
    fn test_down_alert_threshold_default() {
        let settings: Settings = "monitors = []".parse().expect("Failed to parse TOML");
        assert_eq!(settings.down_alert_threshold, 5);
    }

    #[test]
    fn test_down_alert_threshold_custom() {
        let settings: Settings = "monitors = []\ndown_alert_threshold = 10"
            .parse()
            .expect("Failed to parse TOML");
        assert_eq!(settings.down_alert_threshold, 10);
    }

    #[test]
    fn test_smtp_config_from_env() {
        std::env::set_var("SMTP_HOST", "smtp.test.com");
        std::env::set_var("SMTP_PORT", "587");
        std::env::set_var("SMTP_USER", "user@test.com");
        std::env::set_var("SMTP_PASS", "secret");
        std::env::set_var("SMTP_FROM", "from@test.com");
        std::env::set_var("SMTP_TO", "ops@test.com,dev@test.com");

        let config = SmtpConfig::from_env().expect("Should produce config");
        assert_eq!(config.smtp_host, "smtp.test.com");
        assert_eq!(config.smtp_port, 587);
        assert_eq!(config.smtp_username, "user@test.com");
        assert_eq!(config.from_address, "from@test.com");
        assert_eq!(config.to_addresses, vec!["ops@test.com", "dev@test.com"]);
        assert!(config.smtp_use_tls);

        for key in &["SMTP_HOST", "SMTP_PORT", "SMTP_USER", "SMTP_PASS", "SMTP_FROM", "SMTP_TO"] {
            std::env::remove_var(key);
        }
    }
}
