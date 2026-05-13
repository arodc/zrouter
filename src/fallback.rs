use std::time::Duration;

use bytes::Bytes;

use crate::config::{FallbackConfig, RouteConfig};
use crate::error_map::ProviderType;
use crate::provider::{HealthState, Registry};

pub struct FallbackExecutor<'a> {
    pub route: &'a RouteConfig,
    pub registry: &'a Registry,
    pub fallback_config: &'a FallbackConfig,
    pub original_model: String,
}

#[derive(Debug)]
#[allow(dead_code)] // body fields kept for debugging/future use
pub enum AttemptOutcome {
    Success {
        provider_name: String,
        status: u16,
        body: String,
    },
    RetryableFailure {
        status: u16,
        body: String,
        error_type: Option<String>,
        description: Option<String>,
    },
    NonRetryableFailure {
        status: u16,
        body: String,
        error_type: Option<String>,
        description: Option<String>,
    },
    Fatal {
        status: u16,
        body: String,
    },
}

#[derive(Clone)]
pub struct AttemptParams {
    pub provider_name: String,
    pub endpoint: String,
    pub api_key: String,
    pub connect_timeout_secs: u64,
    pub step_model: Option<String>,
    pub provider_type: ProviderType,
}

#[derive(Debug)]
pub struct FallbackResult {
    pub provider_name: String,
    pub status: u16,
    pub body: String,
}

impl<'a> FallbackExecutor<'a> {
    pub async fn execute<F, Fut>(
        &self,
        mut attempt_fn: F,
        original_body: Bytes,
    ) -> Result<FallbackResult, String>
    where
        F: FnMut(AttemptParams, Bytes) -> Fut,
        Fut: std::future::Future<Output = AttemptOutcome>,
    {
        for (step_idx, step) in self.route.steps.iter().enumerate() {
            let provider = match self.registry.get(&step.provider) {
                Some(p) => p.clone(),
                None => {
                    tracing::warn!(provider = %step.provider, "Unknown provider, skipping");
                    continue;
                }
            };

            // Skip unhealthy providers that haven't reached their next probe time
            let health = provider.health_state();
            match &health {
                HealthState::Unhealthy { next_probe_at, .. } => {
                    if crate::provider::epoch_secs() < *next_probe_at {
                        tracing::info!(
                            provider = %step.provider,
                            "Skipping (unhealthy, next probe at {})",
                            next_probe_at
                        );
                        continue;
                    }
                }
                HealthState::Healthy => {}
            }

            let was_unhealthy = !matches!(health, HealthState::Healthy);
            let mut delay_ms = self.fallback_config.initial_delay_ms;

            let params = AttemptParams {
                provider_name: provider.name.clone(),
                endpoint: provider.endpoint.clone(),
                api_key: provider.api_key.clone(),
                connect_timeout_secs: provider.connect_timeout.as_secs(),
                step_model: step.model.clone(),
                provider_type: provider.provider_type,
            };

            if step_idx > 0 {
                tracing::info!(
                    provider = %step.provider,
                    model = %self.original_model,
                    "Falling back to provider"
                );
            }

            let max_attempts = self.fallback_config.step_max_retries.saturating_add(1);

            for attempt in 0..max_attempts {
                if attempt > 0 {
                    tracing::info!(
                        provider = %step.provider,
                        model = %self.original_model,
                        attempt = attempt + 1,
                        "Retrying request"
                    );
                }

                let body = original_body.clone();
                match attempt_fn(params.clone(), body).await {
                    AttemptOutcome::Success {
                        provider_name,
                        status,
                        body,
                    } => {
                        if was_unhealthy {
                            if let Some(old_state) = provider.mark_healthy() {
                                match old_state {
                                    HealthState::Unhealthy {
                                        reason,
                                        failures,
                                        description: desc,
                                        ..
                                    } => {
                                        tracing::info!(
                                            provider = %step.provider,
                                            "Recovered from {} failure ({}, {} retries)",
                                            match reason {
                                                crate::provider::FailureReason::Retryable => "retryable",
                                                crate::provider::FailureReason::NonRetryable => "non-retryable",
                                            },
                                            desc,
                                            failures,
                                        );
                                    }
                                    HealthState::Healthy => {}
                                }
                            }
                        }
                        return Ok(FallbackResult {
                            provider_name,
                            status,
                            body,
                        });
                    }
                    AttemptOutcome::RetryableFailure {
                        status,
                        error_type,
                        description,
                        ..
                    } => {
                        let desc = description
                            .as_deref()
                            .or(error_type.as_deref())
                            .unwrap_or("unknown retryable error");

                        if attempt + 1 < max_attempts {
                            tracing::warn!(
                                provider = %step.provider,
                                status = status,
                                error_type = error_type.as_deref().unwrap_or("N/A"),
                                "Retryable failure: {}, retrying in {}ms",
                                desc,
                                delay_ms,
                            );
                            tokio::time::sleep(Duration::from_millis(delay_ms)).await;
                            delay_ms = (delay_ms * 2).min(self.fallback_config.max_delay_ms);
                        } else {
                            let et = error_type.as_deref().unwrap_or("unknown");
                            tracing::warn!(
                                provider = %step.provider,
                                status = status,
                                error_type = et,
                                "Retryable failure exhausted: {} (HTTP {})\n  Backoff: 8s, next probe at ~now+8s",
                                desc,
                                status,
                            );
                            provider.mark_unhealthy(
                                crate::provider::FailureReason::Retryable,
                                format!("{} (HTTP {})", et, status),
                                self.fallback_config.non_retryable_cooldown_secs,
                            );
                            self.registry.notify_probe();
                            break; // next step
                        }
                    }
                    AttemptOutcome::NonRetryableFailure {
                        status,
                        error_type,
                        description,
                        ..
                    } => {
                        let desc = description
                            .as_deref()
                            .or(error_type.as_deref())
                            .unwrap_or("unknown error");
                        let et = error_type.as_deref().unwrap_or("unknown");
                        tracing::error!(
                            provider = %step.provider,
                            status = status,
                            error_type = et,
                            "Non-retryable failure: {} (HTTP {})\n  Dead until: ~now+{}s",
                            desc,
                            status,
                            self.fallback_config.non_retryable_cooldown_secs,
                        );
                        provider.mark_unhealthy(
                            crate::provider::FailureReason::NonRetryable,
                            format!("{} (HTTP {})", et, status),
                            self.fallback_config.non_retryable_cooldown_secs,
                        );
                        // Notify probe loop so it can schedule a probe
                        self.registry.notify_probe();
                        break; // next step
                    }
                    AttemptOutcome::Fatal { status, body } => {
                        return Err(format!(
                            r#"{{"type":"error","error":{{"type":"api_error","message":"Provider {} returned HTTP {}","upstream_body":{}}}}}"#,
                            step.provider,
                            status,
                            serde_json::Value::String(body)
                        ));
                    }
                }
            }
        }

        Err(
            r#"{"type":"error","error":{"type":"overloaded_error","message":"All providers exhausted"}}"#
                .to_string(),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::FallbackConfig;
    use crate::provider::{FailureReason, HealthState, Provider, Registry};
    use std::collections::HashMap;
    use std::sync::{Arc, RwLock};
    use std::time::Duration;

    fn make_fallback_config() -> FallbackConfig {
        FallbackConfig {
            step_max_retries: 2,
            initial_delay_ms: 10,
            max_delay_ms: 100,
            non_retryable_cooldown_secs: 3600,
        }
    }

    fn make_provider(name: &str, health: HealthState) -> Arc<Provider> {
        Arc::new(Provider {
            name: name.to_string(),
            endpoint: "https://example.com".to_string(),
            api_key: "key".to_string(),
            connect_timeout: Duration::from_secs(10),
            read_timeout: Duration::from_secs(300),
            provider_type: ProviderType::Anthropic,
            health: RwLock::new(health),
        })
    }

    fn make_registry(providers: Vec<Arc<Provider>>) -> Registry {
        let mut map = HashMap::new();
        for p in providers {
            map.insert(p.name.clone(), p);
        }
        Registry {
            providers: map,
            probe_notify: None,
        }
    }

    fn make_route(providers: &[&str]) -> RouteConfig {
        RouteConfig {
            model: "test-model".to_string(),
            steps: providers
                .iter()
                .map(|&p| crate::config::RouteStep {
                    provider: p.to_string(),
                    model: None,
                })
                .collect(),
            debug: crate::config::DebugLevel::default(),
        }
    }

    #[tokio::test]
    async fn test_success_first_attempt() {
        let provider = make_provider("a", HealthState::Healthy);
        let registry = make_registry(vec![provider.clone()]);
        let route = make_route(&["a"]);
        let config = make_fallback_config();

        let executor = FallbackExecutor {
            route: &route,
            registry: &registry,
            fallback_config: &config,
            original_model: "test".to_string(),
        };

        let result = executor
            .execute(
                |_params, _body| async {
                    AttemptOutcome::Success {
                        provider_name: "a".to_string(),
                        status: 200,
                        body: "ok".to_string(),
                    }
                },
                Bytes::from("body"),
            )
            .await;

        assert!(result.is_ok());
        let r = result.unwrap();
        assert_eq!(r.provider_name, "a");
        assert_eq!(r.status, 200);
    }

    #[tokio::test]
    async fn test_retryable_then_success() {
        let provider = make_provider("a", HealthState::Healthy);
        let registry = make_registry(vec![provider.clone()]);
        let route = make_route(&["a"]);
        let config = make_fallback_config();

        let executor = FallbackExecutor {
            route: &route,
            registry: &registry,
            fallback_config: &config,
            original_model: "test".to_string(),
        };

        let call_count = std::cell::Cell::new(0u32);
        let result = executor
            .execute(
                move |_params, _body| {
                    let count = call_count.get() + 1;
                    call_count.set(count);
                    async move {
                        if count == 1 {
                            AttemptOutcome::RetryableFailure {
                                status: 429,
                                body: "rate limited".to_string(),
                                error_type: Some("rate_limit_error".to_string()),
                                description: None,
                            }
                        } else {
                            AttemptOutcome::Success {
                                provider_name: "a".to_string(),
                                status: 200,
                                body: "ok".to_string(),
                            }
                        }
                    }
                },
                Bytes::from("body"),
            )
            .await;

        assert!(result.is_ok());
        assert_eq!(result.unwrap().status, 200);
    }

    #[tokio::test]
    async fn test_retryable_exhausted_falls_back() {
        let provider_a = make_provider("a", HealthState::Healthy);
        let provider_b = make_provider("b", HealthState::Healthy);
        let registry = make_registry(vec![provider_a.clone(), provider_b.clone()]);
        let route = make_route(&["a", "b"]);
        let config = make_fallback_config();

        let executor = FallbackExecutor {
            route: &route,
            registry: &registry,
            fallback_config: &config,
            original_model: "test".to_string(),
        };

        let result = executor
            .execute(
                move |params, _body| {
                    let provider_name = params.provider_name.clone();
                    async move {
                        if provider_name == "a" {
                            AttemptOutcome::RetryableFailure {
                                status: 429,
                                body: "rate limited".to_string(),
                                error_type: Some("rate_limit_error".to_string()),
                                description: None,
                            }
                        } else {
                            AttemptOutcome::Success {
                                provider_name: "b".to_string(),
                                status: 200,
                                body: "ok".to_string(),
                            }
                        }
                    }
                },
                Bytes::from("body"),
            )
            .await;

        assert!(result.is_ok());
        assert_eq!(result.unwrap().provider_name, "b");
        // Provider a should be marked unhealthy
        assert!(matches!(provider_a.health_state(), HealthState::Unhealthy { .. }));
    }

    #[tokio::test]
    async fn test_non_retryable_immediate_fallback() {
        let provider_a = make_provider("a", HealthState::Healthy);
        let provider_b = make_provider("b", HealthState::Healthy);
        let registry = make_registry(vec![provider_a.clone(), provider_b.clone()]);
        let route = make_route(&["a", "b"]);
        let config = make_fallback_config();

        let executor = FallbackExecutor {
            route: &route,
            registry: &registry,
            fallback_config: &config,
            original_model: "test".to_string(),
        };

        let result = executor
            .execute(
                move |params, _body| {
                    let provider_name = params.provider_name.clone();
                    async move {
                        if provider_name == "a" {
                            AttemptOutcome::NonRetryableFailure {
                                status: 401,
                                body: "auth error".to_string(),
                                error_type: Some("authentication_error".to_string()),
                                description: None,
                            }
                        } else {
                            AttemptOutcome::Success {
                                provider_name: "b".to_string(),
                                status: 200,
                                body: "ok".to_string(),
                            }
                        }
                    }
                },
                Bytes::from("body"),
            )
            .await;

        assert!(result.is_ok());
        assert_eq!(result.unwrap().provider_name, "b");
        // Provider a should be marked non-retryable unhealthy
        match provider_a.health_state() {
            HealthState::Unhealthy { reason, .. } => {
                assert_eq!(reason, FailureReason::NonRetryable);
            }
            HealthState::Healthy => panic!("expected Unhealthy"),
        }
    }

    #[tokio::test]
    async fn test_fatal_aborts_immediately() {
        let provider_a = make_provider("a", HealthState::Healthy);
        let provider_b = make_provider("b", HealthState::Healthy);
        let registry = make_registry(vec![provider_a.clone(), provider_b.clone()]);
        let route = make_route(&["a", "b"]);
        let config = make_fallback_config();

        let executor = FallbackExecutor {
            route: &route,
            registry: &registry,
            fallback_config: &config,
            original_model: "test".to_string(),
        };

        let result = executor
            .execute(
                move |_params, _body| {
                    async move {
                        AttemptOutcome::Fatal {
                            status: 405,
                            body: "method not allowed".to_string(),
                        }
                    }
                },
                Bytes::from("body"),
            )
            .await;

        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.contains("HTTP 405"));
    }

    #[tokio::test]
    async fn test_skip_unhealthy_provider() {
        let now = crate::provider::epoch_secs();
        let provider_a = make_provider(
            "a",
            HealthState::Unhealthy {
                reason: FailureReason::Retryable,
                failures: 1,
                next_probe_at: now.saturating_add(9999),
                description: "error".to_string(),
            },
        );
        let provider_b = make_provider("b", HealthState::Healthy);
        let registry = make_registry(vec![provider_a.clone(), provider_b.clone()]);
        let route = make_route(&["a", "b"]);
        let config = make_fallback_config();

        let executor = FallbackExecutor {
            route: &route,
            registry: &registry,
            fallback_config: &config,
            original_model: "test".to_string(),
        };

        let result = executor
            .execute(
                move |params, _body| {
                    let name = params.provider_name.clone();
                    async move {
                        AttemptOutcome::Success {
                            provider_name: name,
                            status: 200,
                            body: "ok".to_string(),
                        }
                    }
                },
                Bytes::from("body"),
            )
            .await;

        assert!(result.is_ok());
        assert_eq!(result.unwrap().provider_name, "b");
    }

    #[tokio::test]
    async fn test_recovery_log_on_success() {
        let provider = make_provider("a", HealthState::Healthy);
        // Manually set to unhealthy first
        provider.mark_unhealthy(
            FailureReason::Retryable,
            "test error".to_string(),
            3600,
        );
        // Set next_probe_at to past so it's not skipped
        {
            let mut guard = provider.health.write().expect("lock");
            if let HealthState::Unhealthy { ref mut next_probe_at, .. } = *guard {
                *next_probe_at = crate::provider::epoch_secs().saturating_sub(1);
            }
        }

        let registry = make_registry(vec![provider.clone()]);
        let route = make_route(&["a"]);
        let config = make_fallback_config();

        let executor = FallbackExecutor {
            route: &route,
            registry: &registry,
            fallback_config: &config,
            original_model: "test".to_string(),
        };

        let result = executor
            .execute(
                move |_params, _body| {
                    async move {
                        AttemptOutcome::Success {
                            provider_name: "a".to_string(),
                            status: 200,
                            body: "ok".to_string(),
                        }
                    }
                },
                Bytes::from("body"),
            )
            .await;

        assert!(result.is_ok());
        // Should be healthy again
        assert!(matches!(provider.health_state(), HealthState::Healthy));
    }

    #[tokio::test]
    async fn test_all_exhausted_returns_error() {
        let provider_a = make_provider("a", HealthState::Healthy);
        let registry = make_registry(vec![provider_a.clone()]);
        let route = make_route(&["a"]);
        let config = make_fallback_config();

        let executor = FallbackExecutor {
            route: &route,
            registry: &registry,
            fallback_config: &config,
            original_model: "test".to_string(),
        };

        let result = executor
            .execute(
                move |_params, _body| {
                    async move {
                        AttemptOutcome::RetryableFailure {
                            status: 500,
                            body: "error".to_string(),
                            error_type: None,
                            description: None,
                        }
                    }
                },
                Bytes::from("body"),
            )
            .await;

        assert!(result.is_err());
        assert!(result.unwrap_err().contains("overloaded_error"));
    }

    #[tokio::test]
    async fn test_attempt_params_has_provider_type() {
        let provider = make_provider("a", HealthState::Healthy);
        let registry = make_registry(vec![provider.clone()]);
        let route = make_route(&["a"]);
        let config = make_fallback_config();

        let executor = FallbackExecutor {
            route: &route,
            registry: &registry,
            fallback_config: &config,
            original_model: "test".to_string(),
        };

        let result = executor
            .execute(
                move |params, _body| {
                    let pt = params.provider_type;
                    async move {
                        assert_eq!(pt, ProviderType::Anthropic);
                        AttemptOutcome::Success {
                            provider_name: "a".to_string(),
                            status: 200,
                            body: "ok".to_string(),
                        }
                    }
                },
                Bytes::from("body"),
            )
            .await;

        assert!(result.is_ok());
    }
}
