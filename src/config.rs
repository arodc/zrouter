use std::collections::HashMap;

use serde::Deserialize;

#[derive(Debug, Deserialize)]
pub struct Config {
    pub server: ServerConfig,
    pub logging: LoggingConfig,
    pub providers: HashMap<String, ProviderConfig>,
    pub route: Vec<RouteConfig>,
    pub fallback: FallbackConfig,
}

#[derive(Debug, Deserialize)]
pub struct ServerConfig {
    pub bind: String,
    pub port: u16,
    pub api_key: Option<String>,
    #[serde(default = "default_max_body_size")]
    pub max_body_size: usize,
    #[serde(default)]
    pub tls: bool,
    pub cert_file: Option<String>,
    pub key_file: Option<String>,
}

fn default_max_body_size() -> usize {
    10 * 1024 * 1024 // 10MB
}

#[derive(Debug, Deserialize)]
pub struct LoggingConfig {
    #[serde(default = "default_log_level")]
    pub level: String,
    #[serde(default = "default_log_format")]
    pub format: String,
}

fn default_log_level() -> String {
    "info".to_string()
}

fn default_log_format() -> String {
    "json".to_string()
}

#[derive(Debug, Deserialize, Clone)]
pub struct ProviderConfig {
    pub endpoint: String,
    pub api_key_env: Option<String>,
    pub api_key: Option<String>,
    #[serde(default = "default_connect_timeout")]
    pub connect_timeout_secs: u64,
    #[serde(default = "default_read_timeout")]
    pub read_timeout_secs: u64,
}

fn default_connect_timeout() -> u64 {
    10
}

fn default_read_timeout() -> u64 {
    300
}

#[derive(Debug, Deserialize, Clone)]
pub struct RouteConfig {
    pub model: String,
    pub steps: Vec<RouteStep>,
}

#[derive(Debug, Deserialize, Clone)]
pub struct RouteStep {
    pub provider: String,
    pub model: Option<String>,
}

#[derive(Debug, Deserialize, Clone)]
pub struct FallbackConfig {
    #[serde(default = "default_trigger_codes")]
    pub trigger_codes: Vec<u16>,
    #[serde(default = "default_max_retries")]
    pub max_retries: u32,
    #[serde(default = "default_initial_delay")]
    pub initial_delay_ms: u64,
    #[serde(default = "default_max_delay")]
    pub max_delay_ms: u64,
    #[serde(default = "default_cb_threshold")]
    pub circuit_breaker_threshold: u32,
    #[serde(default = "default_cb_cooldown")]
    pub circuit_breaker_cooldown_secs: u64,
}

fn default_trigger_codes() -> Vec<u16> {
    vec![429, 500, 502, 503, 504, 529]
}

fn default_max_retries() -> u32 {
    3
}

fn default_initial_delay() -> u64 {
    500
}

fn default_max_delay() -> u64 {
    8000
}

fn default_cb_threshold() -> u32 {
    5
}

fn default_cb_cooldown() -> u64 {
    60
}

impl Default for FallbackConfig {
    fn default() -> Self {
        Self {
            trigger_codes: default_trigger_codes(),
            max_retries: default_max_retries(),
            initial_delay_ms: default_initial_delay(),
            max_delay_ms: default_max_delay(),
            circuit_breaker_threshold: default_cb_threshold(),
            circuit_breaker_cooldown_secs: default_cb_cooldown(),
        }
    }
}

pub fn load(path: &str) -> Result<Config, Box<dyn std::error::Error + Send + Sync>> {
    let content = std::fs::read_to_string(path)?;
    let config: Config = toml::from_str(&content)?;
    validate(&config)?;
    Ok(config)
}

fn validate(config: &Config) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    for route in &config.route {
        for step in &route.steps {
            if !config.providers.contains_key(&step.provider) {
                return Err(format!(
                    "Route '{}' references unknown provider '{}'",
                    route.model, step.provider
                )
                .into());
            }
        }
    }

    if config.route.is_empty() {
        return Err("No routes defined".into());
    }

    if config.server.tls {
        let has_cert = config.server.cert_file.is_some();
        let has_key = config.server.key_file.is_some();
        if has_cert != has_key {
            return Err("Both cert_file and key_file must be specified together, or neither (for auto-generated dev cert)".into());
        }
    }

    Ok(())
}
