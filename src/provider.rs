use std::collections::HashMap;
use std::sync::atomic::{AtomicU32, AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use crate::config::{Config, ProviderConfig};

#[derive(Debug, PartialEq)]
pub enum CircuitState {
    Closed,
    Open,
    HalfOpen,
}

fn epoch_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

pub struct Provider {
    pub name: String,
    pub endpoint: String,
    pub api_key: String,
    pub connect_timeout: Duration,
    #[allow(dead_code)]
    pub read_timeout: Duration,
    consecutive_failures: AtomicU32,
    circuit_opened_epoch: AtomicU64,
}

impl Provider {
    pub fn circuit_state(&self, threshold: u32, cooldown: Duration) -> CircuitState {
        let failures = self.consecutive_failures.load(Ordering::Relaxed);
        if failures < threshold {
            return CircuitState::Closed;
        }

        let opened_epoch = self.circuit_opened_epoch.load(Ordering::Relaxed);
        if opened_epoch == 0 {
            return CircuitState::Closed;
        }

        let now = epoch_secs();
        if now.saturating_sub(opened_epoch) < cooldown.as_secs() {
            return CircuitState::Open;
        }

        CircuitState::HalfOpen
    }

    pub fn record_success(&self) {
        self.consecutive_failures.store(0, Ordering::Relaxed);
        self.circuit_opened_epoch.store(0, Ordering::Relaxed);
    }

    pub fn record_failure(&self, threshold: u32) {
        let failures = self.consecutive_failures.fetch_add(1, Ordering::Relaxed) + 1;
        if failures >= threshold {
            self.circuit_opened_epoch
                .store(epoch_secs(), Ordering::Relaxed);
            tracing::warn!(
                provider = %self.name,
                failures = failures,
                "Circuit breaker tripped"
            );
        }
    }
}

pub struct Registry {
    providers: HashMap<String, Arc<Provider>>,
}

impl Registry {
    pub fn new(config: &Config) -> Result<Self, Box<dyn std::error::Error + Send + Sync>> {
        let mut providers = HashMap::new();

        for (name, pc) in &config.providers {
            let api_key = resolve_api_key(pc, name)?;
            let provider = Provider {
                name: name.clone(),
                endpoint: pc.endpoint.trim_end_matches('/').to_string(),
                api_key,
                connect_timeout: Duration::from_secs(pc.connect_timeout_secs),
                read_timeout: Duration::from_secs(pc.read_timeout_secs),
                consecutive_failures: AtomicU32::new(0),
                circuit_opened_epoch: AtomicU64::new(0),
            };
            providers.insert(name.clone(), Arc::new(provider));
        }

        Ok(Self { providers })
    }

    pub fn get(&self, name: &str) -> Option<&Arc<Provider>> {
        self.providers.get(name)
    }
}

fn resolve_api_key(
    pc: &ProviderConfig,
    name: &str,
) -> Result<String, Box<dyn std::error::Error + Send + Sync>> {
    if let Some(ref env_var) = pc.api_key_env {
        if let Ok(key) = std::env::var(env_var) {
            return Ok(key);
        }
    }

    if let Some(ref key) = pc.api_key {
        return Ok(key.clone());
    }

    Err(format!(
        "Provider '{}': no API key configured (set {} or api_key)",
        name,
        pc.api_key_env.as_deref().unwrap_or("N/A")
    )
    .into())
}
