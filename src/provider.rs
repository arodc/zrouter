use std::collections::HashMap;
use std::sync::{Arc, RwLock};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use tokio::sync::Notify;

use crate::config::{Config, ProviderConfig};
use crate::error_map::ProviderType;

pub(crate) fn epoch_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

#[derive(Debug, Clone, PartialEq)]
pub enum FailureReason {
    Retryable,
    NonRetryable,
}

#[derive(Debug, Clone)]
pub enum HealthState {
    Healthy,
    Unhealthy {
        reason: FailureReason,
        failures: u32,
        next_probe_at: u64,
        description: String,
    },
}

pub struct Provider {
    pub name: String,
    pub endpoint: String,
    pub api_key: String,
    pub connect_timeout: Duration,
    #[allow(dead_code)]
    pub read_timeout: Duration,
    pub provider_type: ProviderType,
    pub(crate) health: RwLock<HealthState>,
}

impl Provider {
    pub fn health_state(&self) -> HealthState {
        let guard = self.health.read().expect("provider health lock poisoned");
        guard.clone()
    }

    pub fn mark_unhealthy(
        &self,
        reason: FailureReason,
        description: String,
        non_retryable_cooldown: u64,
    ) {
        let now = epoch_secs();
        let next_probe_at = match reason {
            FailureReason::Retryable => now.saturating_add(8),
            FailureReason::NonRetryable => now.saturating_add(non_retryable_cooldown),
        };

        let mut guard = self.health.write().expect("provider health lock poisoned");
        let failures = match &*guard {
            HealthState::Unhealthy { failures, .. } => failures.saturating_add(1),
            _ => 1,
        };
        *guard = HealthState::Unhealthy {
            reason,
            failures,
            next_probe_at,
            description,
        };
    }

    /// Mark provider as healthy. Returns the previous HealthState if it was Unhealthy.
    pub fn mark_healthy(&self) -> Option<HealthState> {
        let mut guard = self.health.write().expect("provider health lock poisoned");
        match &*guard {
            HealthState::Healthy => None,
            HealthState::Unhealthy { .. } => {
                let old = guard.clone();
                *guard = HealthState::Healthy;
                Some(old)
            }
        }
    }

    pub fn record_probe_failure(&self, reason: FailureReason, cooldown: u64) {
        let now = epoch_secs();
        let mut guard = self.health.write().expect("provider health lock poisoned");

        let (failures, old_reason) = match &*guard {
            HealthState::Unhealthy { failures, reason, .. } => (failures.saturating_add(1), reason.clone()),
            HealthState::Healthy => (1, reason.clone()),
        };

        let next_probe_at = match old_reason {
            FailureReason::Retryable => {
                let shift = failures.saturating_sub(1).min(6) as u32; // cap shift at 6 (64s base)
                let multiplier = 1u64 << shift;
                let backoff = 8u64.saturating_mul(multiplier);
                now.saturating_add(backoff.min(900))
            }
            FailureReason::NonRetryable => now.saturating_add(cooldown),
        };

        *guard = HealthState::Unhealthy {
            reason: old_reason,
            failures,
            next_probe_at,
            description: "probe failure".to_string(),
        };
    }
}

pub struct Registry {
    pub(crate) providers: HashMap<String, Arc<Provider>>,
    pub(crate) probe_notify: Option<Arc<Notify>>,
}

impl Registry {
    pub fn new(
        config: &Config,
        probe_notify: Option<Arc<Notify>>,
    ) -> Result<Self, Box<dyn std::error::Error + Send + Sync>> {
        let mut providers = HashMap::new();

        for (name, pc) in &config.providers {
            let api_key = resolve_api_key(pc, name)?;
            let provider_type = ProviderType::from_str_lossy(&pc.provider_type);
            let provider = Provider {
                name: name.clone(),
                endpoint: pc.endpoint.trim_end_matches('/').to_string(),
                api_key,
                connect_timeout: Duration::from_secs(pc.connect_timeout_secs),
                read_timeout: Duration::from_secs(pc.read_timeout_secs),
                provider_type,
                health: RwLock::new(HealthState::Healthy),
            };
            providers.insert(name.clone(), Arc::new(provider));
        }

        Ok(Self {
            providers,
            probe_notify,
        })
    }

    pub fn get(&self, name: &str) -> Option<&Arc<Provider>> {
        self.providers.get(name)
    }

    #[allow(dead_code)]
    pub fn unhealthy_providers(&self) -> Vec<(String, Arc<Provider>)> {
        self.providers
            .iter()
            .filter(|(_, p)| !matches!(p.health_state(), HealthState::Healthy))
            .map(|(n, p)| (n.clone(), p.clone()))
            .collect()
    }

    pub fn probe_candidates(&self, now_secs: u64) -> Vec<Arc<Provider>> {
        self.providers
            .iter()
            .filter(|(_, p)| match p.health_state() {
                HealthState::Unhealthy { next_probe_at, .. } => next_probe_at <= now_secs,
                HealthState::Healthy => false,
            })
            .map(|(_, p)| p.clone())
            .collect()
    }

    pub fn min_next_probe(&self) -> Option<u64> {
        self.providers
            .iter()
            .filter_map(|(_, p)| match p.health_state() {
                HealthState::Unhealthy { next_probe_at, .. } => Some(next_probe_at),
                HealthState::Healthy => None,
            })
            .min()
    }

    pub fn notify_probe(&self) {
        if let Some(ref notify) = self.probe_notify {
            notify.notify_one();
        }
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_mark_unhealthy_retryable() {
        let provider = Provider {
            name: "test".to_string(),
            endpoint: "https://example.com".to_string(),
            api_key: "key".to_string(),
            connect_timeout: Duration::from_secs(10),
            read_timeout: Duration::from_secs(300),
            provider_type: ProviderType::Anthropic,
            health: RwLock::new(HealthState::Healthy),
        };

        provider.mark_unhealthy(
            FailureReason::Retryable,
            "rate limited".to_string(),
            3600,
        );

        match provider.health_state() {
            HealthState::Unhealthy { reason, failures, .. } => {
                assert_eq!(reason, FailureReason::Retryable);
                assert_eq!(failures, 1);
            }
            HealthState::Healthy => panic!("expected Unhealthy"),
        }
    }

    #[test]
    fn test_mark_unhealthy_non_retryable() {
        let provider = Provider {
            name: "test".to_string(),
            endpoint: "https://example.com".to_string(),
            api_key: "key".to_string(),
            connect_timeout: Duration::from_secs(10),
            read_timeout: Duration::from_secs(300),
            provider_type: ProviderType::Anthropic,
            health: RwLock::new(HealthState::Healthy),
        };

        provider.mark_unhealthy(
            FailureReason::NonRetryable,
            "auth error".to_string(),
            3600,
        );

        match provider.health_state() {
            HealthState::Unhealthy { reason, next_probe_at, .. } => {
                assert_eq!(reason, FailureReason::NonRetryable);
                let now = epoch_secs();
                assert!(next_probe_at >= now.saturating_add(3599));
            }
            HealthState::Healthy => panic!("expected Unhealthy"),
        }
    }

    #[test]
    fn test_mark_healthy_returns_old_state() {
        let provider = Provider {
            name: "test".to_string(),
            endpoint: "https://example.com".to_string(),
            api_key: "key".to_string(),
            connect_timeout: Duration::from_secs(10),
            read_timeout: Duration::from_secs(300),
            provider_type: ProviderType::Anthropic,
            health: RwLock::new(HealthState::Healthy),
        };

        assert!(provider.mark_healthy().is_none());

        provider.mark_unhealthy(
            FailureReason::Retryable,
            "error".to_string(),
            3600,
        );
        let old = provider.mark_healthy();
        assert!(old.is_some());
        assert!(matches!(provider.health_state(), HealthState::Healthy));
    }

    #[test]
    fn test_record_probe_failure_retryable_backoff() {
        let provider = Provider {
            name: "test".to_string(),
            endpoint: "https://example.com".to_string(),
            api_key: "key".to_string(),
            connect_timeout: Duration::from_secs(10),
            read_timeout: Duration::from_secs(300),
            provider_type: ProviderType::Anthropic,
            health: RwLock::new(HealthState::Healthy),
        };

        // First failure
        provider.record_probe_failure(FailureReason::Retryable, 3600);
        let now = epoch_secs();
        match provider.health_state() {
            HealthState::Unhealthy { next_probe_at, failures, .. } => {
                assert_eq!(failures, 1);
                // backoff = 8 * 2^0 = 8
                assert!(next_probe_at >= now.saturating_add(8));
                assert!(next_probe_at <= now.saturating_add(10));
            }
            HealthState::Healthy => panic!("expected Unhealthy"),
        }

        // Second failure
        provider.record_probe_failure(FailureReason::Retryable, 3600);
        let now = epoch_secs();
        match provider.health_state() {
            HealthState::Unhealthy { next_probe_at, failures, .. } => {
                assert_eq!(failures, 2);
                // backoff = 8 * 2^1 = 16
                assert!(next_probe_at >= now.saturating_add(16));
                assert!(next_probe_at <= now.saturating_add(18));
            }
            HealthState::Healthy => panic!("expected Unhealthy"),
        }
    }

    #[test]
    fn test_record_probe_failure_non_retryable_fixed_cooldown() {
        let provider = Provider {
            name: "test".to_string(),
            endpoint: "https://example.com".to_string(),
            api_key: "key".to_string(),
            connect_timeout: Duration::from_secs(10),
            read_timeout: Duration::from_secs(300),
            provider_type: ProviderType::Anthropic,
            health: RwLock::new(HealthState::Healthy),
        };

        provider.mark_unhealthy(
            FailureReason::NonRetryable,
            "auth error".to_string(),
            3600,
        );

        // Probe failure for non-retryable: should use fixed cooldown
        provider.record_probe_failure(FailureReason::NonRetryable, 3600);
        let now = epoch_secs();
        match provider.health_state() {
            HealthState::Unhealthy { next_probe_at, failures, .. } => {
                assert_eq!(failures, 2);
                assert!(next_probe_at >= now.saturating_add(3599));
            }
            HealthState::Healthy => panic!("expected Unhealthy"),
        }
    }

    #[test]
    fn test_registry_probe_candidates() {
        let p1 = Arc::new(Provider {
            name: "a".to_string(),
            endpoint: "https://a.com".to_string(),
            api_key: "key".to_string(),
            connect_timeout: Duration::from_secs(10),
            read_timeout: Duration::from_secs(300),
            provider_type: ProviderType::Anthropic,
            health: RwLock::new(HealthState::Unhealthy {
                reason: FailureReason::Retryable,
                failures: 1,
                next_probe_at: epoch_secs().saturating_sub(1), // past due
                description: "error".to_string(),
            }),
        });
        let p2 = Arc::new(Provider {
            name: "b".to_string(),
            endpoint: "https://b.com".to_string(),
            api_key: "key".to_string(),
            connect_timeout: Duration::from_secs(10),
            read_timeout: Duration::from_secs(300),
            provider_type: ProviderType::Anthropic,
            health: RwLock::new(HealthState::Unhealthy {
                reason: FailureReason::Retryable,
                failures: 1,
                next_probe_at: epoch_secs().saturating_add(9999), // future
                description: "error".to_string(),
            }),
        });

        let reg = Registry {
            providers: HashMap::from([
                ("a".to_string(), p1),
                ("b".to_string(), p2),
            ]),
            probe_notify: None,
        };

        let candidates = reg.probe_candidates(epoch_secs());
        assert_eq!(candidates.len(), 1);
        assert_eq!(candidates[0].name, "a");
    }
}
