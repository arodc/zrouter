use std::sync::Arc;
use std::time::Duration;

use http::{Method, Request, Uri};
use tokio::sync::{broadcast, Notify};

use crate::config::FallbackConfig;
use crate::error_map::{Classification, ErrorClassifier};
use crate::provider::{self, FailureReason, HealthState, Registry};

/// Default rescan interval when no unhealthy providers exist (seconds).
const DEFAULT_RESCAN_SECS: u64 = 30;

/// Fixed probe request body — minimal Anthropic-compatible payload.
const PROBE_BODY: &[u8] = br#"{"max_tokens":1,"messages":[{"role":"user","content":"hi"}]}"#;

type HttpClient = hyper_util::client::legacy::Client<
    hyper_rustls::HttpsConnector<hyper_util::client::legacy::connect::HttpConnector>,
    String,
>;

pub async fn run_probe_loop(
    registry: Arc<Registry>,
    http_client: Arc<HttpClient>,
    notify: Arc<Notify>,
    fallback_config: FallbackConfig,
    mut shutdown_rx: broadcast::Receiver<()>,
) {
    loop {
        // Calculate next wake time
        let sleep_until = match registry.min_next_probe() {
            Some(t) => tokio::time::Instant::now() + Duration::from_secs(t.saturating_sub(provider::epoch_secs())),
            None => tokio::time::Instant::now() + Duration::from_secs(DEFAULT_RESCAN_SECS),
        };

        tokio::select! {
            _ = tokio::time::sleep_until(sleep_until) => {}
            _ = notify.notified() => {}
            _ = shutdown_rx.recv() => {
                tracing::info!("Probe loop shutting down");
                break;
            }
        }

        let now = provider::epoch_secs();
        let candidates = registry.probe_candidates(now);

        for provider in candidates {
            let classifier = ErrorClassifier::from_config(&fallback_config, provider.provider_type);
            probe_provider(&provider, &http_client, &classifier, &fallback_config).await;
        }
    }
}

async fn probe_provider(
    provider: &Arc<provider::Provider>,
    client: &HttpClient,
    classifier: &ErrorClassifier,
    fallback_config: &FallbackConfig,
) {
    let url = format!("{}/v1/messages", provider.endpoint);
    let uri: Uri = match url.parse() {
        Ok(u) => u,
        Err(e) => {
            tracing::warn!(provider = %provider.name, error = %e, "Probe: invalid URL");
            return;
        }
    };

    let body_str = String::from_utf8_lossy(PROBE_BODY).to_string();
    let req = match Request::builder()
        .method(Method::POST)
        .uri(uri)
        .header("x-api-key", &provider.api_key)
        .header("content-type", "application/json")
        .header("anthropic-version", "2023-06-01")
        .body(body_str)
    {
        Ok(r) => r,
        Err(e) => {
            tracing::warn!(provider = %provider.name, error = %e, "Probe: failed to build request");
            return;
        }
    };

    let response = match tokio::time::timeout(
        // Ensure probe has at least 10s timeout; provider connect_timeout may be shorter
        Duration::from_secs(provider.connect_timeout.as_secs().max(10)),
        client.request(req),
    )
    .await
    {
        Ok(Ok(resp)) => resp,
        Ok(Err(e)) => {
            tracing::warn!(provider = %provider.name, error = %e, "Probe: connection error");
            provider.record_probe_failure(FailureReason::Retryable, fallback_config.non_retryable_cooldown_secs);
            return;
        }
        Err(_) => {
            tracing::warn!(provider = %provider.name, "Probe: timeout");
            provider.record_probe_failure(FailureReason::Retryable, fallback_config.non_retryable_cooldown_secs);
            return;
        }
    };

    let status = response.status().as_u16();
    let body_text = read_probe_body(response.into_body()).await.unwrap_or_default();

    match classifier.classify(status, &body_text) {
        Classification::Success => {
            if let Some(old_state) = provider.mark_healthy() {
                match old_state {
                    HealthState::Unhealthy {
                        reason,
                        failures,
                        description,
                        ..
                    } => {
                        let recovered_label = match reason {
                            FailureReason::NonRetryable => {
                                format!("recovered after non-retryable cooldown ({})", description)
                            }
                            FailureReason::Retryable => {
                                format!("recovered after backoff ({} retryable failures)", failures)
                            }
                        };
                        tracing::info!(
                            provider = %provider.name,
                            "Probe: provider {}",
                            recovered_label,
                        );
                    }
                    HealthState::Healthy => {}
                }
            }
        }
        Classification::Retryable { error_type, description } => {
            let desc = description
                .map(|s| s.to_string())
                .or(error_type)
                .unwrap_or_else(|| format!("HTTP {}", status));
            tracing::warn!(
                provider = %provider.name,
                status = status,
                "Probe: retryable failure: {}",
                desc,
            );
            provider.record_probe_failure(FailureReason::Retryable, fallback_config.non_retryable_cooldown_secs);
        }
        Classification::NonRetryable { error_type, description } => {
            let desc = description
                .map(|s| s.to_string())
                .or(error_type)
                .unwrap_or_else(|| format!("HTTP {}", status));
            tracing::error!(
                provider = %provider.name,
                status = status,
                "Probe: non-retryable failure: {}",
                desc,
            );
            provider.record_probe_failure(FailureReason::NonRetryable, fallback_config.non_retryable_cooldown_secs);
        }
        Classification::Fatal => {
            tracing::error!(
                provider = %provider.name,
                status = status,
                "Probe: fatal error (HTTP {})",
                status,
            );
            provider.record_probe_failure(FailureReason::NonRetryable, fallback_config.non_retryable_cooldown_secs);
        }
    }
}

async fn read_probe_body(
    body: hyper::body::Incoming,
) -> Result<String, Box<dyn std::error::Error + Send + Sync>> {
    use http_body_util::BodyExt;
    let collected = body.collect().await?;
    let bytes = collected.to_bytes();
    // Limit response body to 1MB for probe responses
    if bytes.len() > 1024 * 1024 {
        return Err("Probe response body too large".into());
    }
    Ok(String::from_utf8_lossy(&bytes).to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::FallbackConfig;
    use crate::error_map::ProviderType;
    use crate::provider::{HealthState, Provider};
    use crate::server;
    use std::collections::HashMap;
    use std::sync::RwLock;
    use std::time::Duration;

    fn make_test_provider(name: &str, health: HealthState) -> Arc<Provider> {
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

    #[test]
    fn test_probe_candidates_filters_by_time() {
        let now = provider::epoch_secs();
        let past = make_test_provider(
            "past",
            HealthState::Unhealthy {
                reason: FailureReason::Retryable,
                failures: 1,
                next_probe_at: now.saturating_sub(1),
                description: "error".to_string(),
            },
        );
        let future = make_test_provider(
            "future",
            HealthState::Unhealthy {
                reason: FailureReason::Retryable,
                failures: 1,
                next_probe_at: now.saturating_add(9999),
                description: "error".to_string(),
            },
        );
        let healthy = make_test_provider("healthy", HealthState::Healthy);

        let reg = Registry {
            providers: HashMap::from([
                ("past".to_string(), past),
                ("future".to_string(), future),
                ("healthy".to_string(), healthy),
            ]),
            probe_notify: None,
        };

        let candidates = reg.probe_candidates(now);
        assert_eq!(candidates.len(), 1);
        assert_eq!(candidates[0].name, "past");
    }

    #[test]
    fn test_min_next_probe_returns_earliest() {
        let now = provider::epoch_secs();
        let p1 = make_test_provider(
            "a",
            HealthState::Unhealthy {
                reason: FailureReason::Retryable,
                failures: 1,
                next_probe_at: now.saturating_add(100),
                description: "error".to_string(),
            },
        );
        let p2 = make_test_provider(
            "b",
            HealthState::Unhealthy {
                reason: FailureReason::Retryable,
                failures: 1,
                next_probe_at: now.saturating_add(50),
                description: "error".to_string(),
            },
        );

        let reg = Registry {
            providers: HashMap::from([
                ("a".to_string(), p1),
                ("b".to_string(), p2),
            ]),
            probe_notify: None,
        };

        let min = reg.min_next_probe();
        assert!(min.is_some());
        assert!(min.unwrap() <= now.saturating_add(50));
    }

    #[test]
    fn test_min_next_probe_none_when_all_healthy() {
        let p = make_test_provider("a", HealthState::Healthy);
        let reg = Registry {
            providers: HashMap::from([("a".to_string(), p)]),
            probe_notify: None,
        };
        assert!(reg.min_next_probe().is_none());
    }

    #[test]
    fn test_notify_probe_wakes() {
        let notify = Arc::new(Notify::new());
        let reg = Registry {
            providers: HashMap::new(),
            probe_notify: Some(notify.clone()),
        };

        // Should not panic
        reg.notify_probe();
    }

    #[tokio::test]
    async fn test_probe_loop_shutdown() {
        let (tx, rx) = tokio::sync::broadcast::channel(1);
        let notify = Arc::new(Notify::new());
        let registry = Arc::new(Registry {
            providers: HashMap::new(),
            probe_notify: Some(notify.clone()),
        });
        let client = Arc::new(server::build_http_client());
        let config = FallbackConfig::default();

        // Send shutdown immediately
        tx.send(()).unwrap();

        run_probe_loop(registry, client, notify, config, rx).await;
        // If we get here, the loop exited on shutdown
    }
}
