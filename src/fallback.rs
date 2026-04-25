use std::time::Duration;

use bytes::Bytes;

use crate::config::{FallbackConfig, RouteConfig};
use crate::provider::{CircuitState, Registry};

pub struct FallbackExecutor<'a> {
    pub route: &'a RouteConfig,
    pub registry: &'a Registry,
    pub fallback_config: &'a FallbackConfig,
}

pub enum AttemptOutcome {
    Success {
        provider_name: String,
        status: u16,
        body: String,
    },
    Retryable {
        status: u16,
        body: String,
    },
    Fatal {
        status: u16,
        body: String,
    },
}

pub struct AttemptParams {
    pub provider_name: String,
    pub endpoint: String,
    pub api_key: String,
    pub connect_timeout_secs: u64,
    pub step_model: Option<String>,
}

impl Clone for AttemptParams {
    fn clone(&self) -> Self {
        Self {
            provider_name: self.provider_name.clone(),
            endpoint: self.endpoint.clone(),
            api_key: self.api_key.clone(),
            connect_timeout_secs: self.connect_timeout_secs,
            step_model: self.step_model.clone(),
        }
    }
}

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
        let mut last_status: u16 = 503;

        for step in &self.route.steps {
            let provider = match self.registry.get(&step.provider) {
                Some(p) => p.clone(),
                None => {
                    tracing::warn!(provider = %step.provider, "Unknown provider, skipping");
                    continue;
                }
            };

            let state = provider.circuit_state(
                self.fallback_config.circuit_breaker_threshold,
                Duration::from_secs(self.fallback_config.circuit_breaker_cooldown_secs),
            );

            if state == CircuitState::Open {
                tracing::info!(provider = %step.provider, "Skipping (circuit breaker open)");
                continue;
            }

            // Reset delay when switching to a new step
            let mut delay_ms = self.fallback_config.initial_delay_ms;

            let params = AttemptParams {
                provider_name: provider.name.clone(),
                endpoint: provider.endpoint.clone(),
                api_key: provider.api_key.clone(),
                connect_timeout_secs: provider.connect_timeout.as_secs(),
                step_model: step.model.clone(),
            };

            for attempt in 0..self.fallback_config.max_retries {
                tracing::info!(
                    provider = %step.provider,
                    model = ?step.model,
                    attempt = attempt + 1,
                    "Attempting request"
                );

                let body = original_body.clone();
                match attempt_fn(params.clone(), body).await {
                    AttemptOutcome::Success {
                        provider_name,
                        status,
                        body,
                    } => {
                        provider.record_success();
                        return Ok(FallbackResult {
                            provider_name,
                            status,
                            body,
                        });
                    }
                    AttemptOutcome::Retryable { status, body } => {
                        tracing::warn!(
                            provider = %step.provider,
                            status = status,
                            "Retryable error"
                        );
                        provider.record_failure(self.fallback_config.circuit_breaker_threshold);
                        last_status = status;
                        let _ = body;

                        if attempt + 1 < self.fallback_config.max_retries {
                            tokio::time::sleep(Duration::from_millis(delay_ms)).await;
                            delay_ms = (delay_ms * 2).min(self.fallback_config.max_delay_ms);
                        }
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

        Err(format!(
            r#"{{"type":"error","error":{{"type":"overloaded_error","message":"All providers exhausted (last: HTTP {})"}}}}"#,
            last_status
        ))
    }
}

pub fn is_trigger_code(status: u16, trigger_codes: &[u16]) -> bool {
    trigger_codes.contains(&status)
}
