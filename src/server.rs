use std::sync::Arc;

use hyper_rustls::ConfigBuilderExt;

use bytes::Bytes;
use http::{Method, Request, Response, StatusCode, Uri};
use hyper::body::Incoming;
use hyper_util::rt::TokioExecutor;
use tokio::sync::broadcast;

use crate::auth;
use crate::fallback::{self, AttemptOutcome, AttemptParams, FallbackExecutor};
use crate::provider::Registry;
use crate::proxy;
use crate::router;

type HttpClient = hyper_util::client::legacy::Client<
    hyper_rustls::HttpsConnector<hyper_util::client::legacy::connect::HttpConnector>,
    String,
>;

pub struct AppState {
    pub config: crate::config::Config,
    pub providers: Registry,
}

fn build_http_client() -> HttpClient {
    // Custom TLS config: some upstreams (e.g. open.bigmodel.cn) require
    // explicit TLS 1.2+1.3 version negotiation — the default connector
    // may not offer TLS 1.2, causing a "ProtocolVersion" fatal alert.
    let provider = rustls::crypto::ring::default_provider();
    let config = rustls::ClientConfig::builder_with_provider(provider.into())
        .with_protocol_versions(&[
            &rustls::version::TLS12,
            &rustls::version::TLS13,
        ])
        .expect("inconsistent TLS version config")
        .with_webpki_roots()
        .with_no_client_auth();

    let tls =
        hyper_rustls::HttpsConnectorBuilder::new()
            .with_tls_config(config)
            .https_or_http()
            .enable_http1()
            .enable_http2()
            .build();

    hyper_util::client::legacy::Client::builder(TokioExecutor::new()).build(tls)
}

pub async fn serve(
    listener: tokio::net::TcpListener,
    state: Arc<AppState>,
    tls_acceptor: Option<tokio_rustls::TlsAcceptor>,
    shutdown_tx: broadcast::Sender<()>,
) {
    let client = Arc::new(build_http_client());
    let mut shutdown_rx = shutdown_tx.subscribe();

    loop {
        tokio::select! {
            accept_result = listener.accept() => {
                match accept_result {
                    Ok((stream, remote_addr)) => {
                        let state = state.clone();
                        let client = client.clone();
                        let tls_acceptor = tls_acceptor.clone();

                        tokio::spawn(async move {
                            if let Some(acceptor) = tls_acceptor {
                                match acceptor.accept(stream).await {
                                    Ok(tls_stream) => {
                                        let io = hyper_util::rt::TokioIo::new(tls_stream);
                                        serve_connection(io, state, client, remote_addr).await;
                                    }
                                    Err(e) => {
                                        tracing::error!(error = %e, peer = %remote_addr, "TLS handshake failed");
                                    }
                                }
                            } else {
                                let io = hyper_util::rt::TokioIo::new(stream);
                                serve_connection(io, state, client, remote_addr).await;
                            }
                        });
                    }
                    Err(e) => {
                        tracing::error!(error = %e, "Accept error");
                    }
                }
            }
            _ = shutdown_rx.recv() => {
                tracing::info!("Shutdown signal received, stopping");
                break;
            }
        }
    }
}

async fn serve_connection<I>(
    io: I,
    state: Arc<AppState>,
    client: Arc<HttpClient>,
    remote_addr: std::net::SocketAddr,
)
where
    I: hyper::rt::Read + hyper::rt::Write + Unpin + Send + 'static,
{
    let builder = hyper_util::server::conn::auto::Builder::new(TokioExecutor::new());
    let service = hyper::service::service_fn(move |req| {
        let state = state.clone();
        let client = client.clone();
        async move { handle_request(req, &state, &client).await }
    });

    if let Err(e) = builder.serve_connection(io, service).await {
        tracing::error!(error = %e, peer = %remote_addr, "Connection error");
    }
}

async fn handle_request(
    req: Request<Incoming>,
    state: &AppState,
    client: &HttpClient,
) -> Result<Response<String>, http::Error> {
    let trace_id = uuid::Uuid::new_v4();

    if req.method() == Method::GET && req.uri().path() == "/health" {
        return Ok(Response::new(r#"{"status":"ok"}"#.to_string()));
    }

    if req.method() != Method::POST || req.uri().path() != "/v1/messages" {
        return make_error(StatusCode::NOT_FOUND, "not_found_error", "Not found");
    }

    let provided_key = req
        .headers()
        .get("x-api-key")
        .and_then(|v| v.to_str().ok());
    if !auth::verify_api_key(provided_key, &state.config.server.api_key) {
        return make_error(
            StatusCode::UNAUTHORIZED,
            "authentication_error",
            "Invalid API key",
        );
    }

    let original_headers = req.headers().clone();

    let body_bytes = match read_body(req.into_body(), state.config.server.max_body_size).await {
        Ok(b) => b,
        Err(e) => {
            tracing::error!(trace_id = %trace_id, error = %e, "Failed to read request body");
            return make_error(
                StatusCode::BAD_REQUEST,
                "invalid_request_error",
                "Failed to read request body",
            );
        }
    };

    let model = match proxy::extract_model(&body_bytes) {
        Some(m) => m,
        None => {
            return make_error(
                StatusCode::BAD_REQUEST,
                "invalid_request_error",
                "Missing 'model' field in request body",
            );
        }
    };

    tracing::info!(trace_id = %trace_id, model = %model, "Request received");

    let route = match router::resolve_route(&state.config.route, &model) {
        Some(r) => r,
        None => {
            return make_error(
                StatusCode::NOT_FOUND,
                "not_found_error",
                &format!("No route found for model '{}'", model),
            );
        }
    };

    let executor = FallbackExecutor {
        route,
        registry: &state.providers,
        fallback_config: &state.config.fallback,
    };

    let trigger_codes = state.config.fallback.trigger_codes.clone();

    let result = executor
        .execute(
            |params, body| {
                let client = client.clone();
                let headers = original_headers.clone();
                let trigger_codes = trigger_codes.clone();
                let body = proxy::replace_model(&body, params.step_model.as_deref());
                async move {
                    upstream_attempt(&client, params, &headers, body, &trigger_codes).await
                }
            },
            body_bytes,
        )
        .await;

    match result {
        Ok(fallback_result) => {
            tracing::info!(
                trace_id = %trace_id,
                provider = %fallback_result.provider_name,
                model = %model,
                status = fallback_result.status,
                "Request completed"
            );

            let status = StatusCode::from_u16(fallback_result.status)
                .unwrap_or(StatusCode::OK);

            Response::builder()
                .status(status)
                .header("content-type", "application/json")
                .header("x-zrouter-provider", &fallback_result.provider_name)
                .body(fallback_result.body)
        }
        Err(error_json) => {
            tracing::warn!(trace_id = %trace_id, model = %model, "All providers exhausted");
            Response::builder()
                .status(StatusCode::SERVICE_UNAVAILABLE)
                .header("content-type", "application/json")
                .body(error_json)
        }
    }
}

async fn upstream_attempt(
    client: &HttpClient,
    params: AttemptParams,
    original_headers: &http::HeaderMap,
    body: Bytes,
    trigger_codes: &[u16],
) -> AttemptOutcome {
    let url = format!("{}/v1/messages", params.endpoint);
    let uri: Uri = match url.parse() {
        Ok(u) => u,
        Err(e) => {
            return AttemptOutcome::Retryable {
                status: 500,
                body: format!("Invalid upstream URL: {}", e),
            };
        }
    };

    let mut req_builder = Request::builder()
        .method(Method::POST)
        .uri(uri)
        .header("x-api-key", &params.api_key)
        .header("content-type", "application/json");

    if let Some(v) = original_headers.get("anthropic-version") {
        req_builder = req_builder.header("anthropic-version", v);
    }
    if let Some(v) = original_headers.get("anthropic-beta") {
        req_builder = req_builder.header("anthropic-beta", v);
    }

    let body_str = String::from_utf8_lossy(&body).to_string();
    let req = match req_builder.body(body_str) {
        Ok(r) => r,
        Err(e) => {
            return AttemptOutcome::Retryable {
                status: 500,
                body: format!("Failed to build request: {}", e),
            };
        }
    };

    let response = match tokio::time::timeout(
        std::time::Duration::from_secs(params.connect_timeout_secs),
        client.request(req),
    )
    .await
    {
        Ok(Ok(resp)) => resp,
        Ok(Err(e)) => {
            tracing::warn!(error = %e, "Upstream connection error");
            return AttemptOutcome::Retryable {
                status: 502,
                body: format!("Upstream connection error: {}", e),
            };
        }
        Err(_) => {
            return AttemptOutcome::Retryable {
                status: 504,
                body: "Upstream connection timeout".to_string(),
            };
        }
    };

    let status = response.status().as_u16();
    let is_success = response.status().is_success();
    let is_trigger = fallback::is_trigger_code(status, trigger_codes);
    let body_text = read_body_string(response.into_body())
        .await
        .unwrap_or_default();

    if is_trigger {
        AttemptOutcome::Retryable {
            status,
            body: body_text,
        }
    } else if is_success {
        AttemptOutcome::Success {
            provider_name: params.provider_name,
            status,
            body: body_text,
        }
    } else {
        AttemptOutcome::Fatal {
            status,
            body: body_text,
        }
    }
}

fn make_error(
    status: StatusCode,
    error_type: &str,
    message: &str,
) -> Result<Response<String>, http::Error> {
    let body = format!(
        r#"{{"type":"error","error":{{"type":"{}","message":"{}"}}}}"#,
        error_type,
        message.replace('"', "\\\"")
    );
    Response::builder()
        .status(status)
        .header("content-type", "application/json")
        .body(body)
}

async fn read_body(
    body: Incoming,
    max_size: usize,
) -> Result<Bytes, Box<dyn std::error::Error + Send + Sync>> {
    use http_body_util::BodyExt;

    let collected = body.collect().await?;
    let bytes = collected.to_bytes();

    if bytes.len() > max_size {
        return Err(format!("Request body too large: {} bytes", bytes.len()).into());
    }

    Ok(bytes)
}

async fn read_body_string(
    body: Incoming,
) -> Result<String, Box<dyn std::error::Error + Send + Sync>> {
    let bytes = read_body(body, 50 * 1024 * 1024).await?;
    Ok(String::from_utf8_lossy(&bytes).to_string())
}
