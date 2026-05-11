# ZRouter Development Log

## 2026-05-12 — vv mode: trim tool definitions to names only

- **`src/debug.rs`**: In vv mode, clone the request body value and replace each tool definition with `{"name": "<tool_name>"}` before pretty-printing. This prevents massive `input_schema` objects from flooding logs. Messages, system prompt, and all other fields remain intact.
- Added test `test_vv_mode_tools_stripped_to_names_only` verifying the trimming logic and that non-tool fields are preserved.

## 2026-05-12 — Debug logging refinements

- **context_size fixes**: Now counts tool_result block content (string or array of text blocks). Previously missed large tool outputs.
- **Response parse robustness**: Empty/whitespace body detected before JSON parse. SSE `data: {...}` format attempted when parse fails.
- **Structured log format**: Replaced multi-line message strings with flat tracing fields (msg_count, user_count, assistant_count, tool_result_count, system, tool_count, tool_names, max_tokens, content_chars). Queries are now field-name-based.
- **Merged `system_length` into `text_or_array_length`** — removed duplicated array-traversal logic.

## 2026-05-12 — Startup version info

- **`build.rs`**: Extracts git commit via `git describe --always --dirty --broken` and sets `GIT_COMMIT` env for compile-time use.
- **`src/main.rs`**: Startup log now includes `CARGO_PKG_VERSION` and `GIT_COMMIT` fields.

## 2026-05-12 — Per-model debug logging

- **`src/config.rs`**: Added `DebugLevel` enum (`None`, `V`, `Vv`) with serde snake_case rename and `Default = None`. Added `debug: DebugLevel` field to `RouteConfig` with `#[serde(default)]`.
- **`src/debug.rs` (NEW)**: `log_request()` and `log_response()` functions parse Anthropic Messages API JSON bodies. V mode logs message counts (by role), system prompt presence/length, tool definition names, max_tokens, approximate context size (request); stop_reason, content block counts, tool call names, usage tokens (response). Vv mode adds pretty-printed full body. All JSON parse failures are caught and logged as warnings — never causes request failure.
- **`src/server.rs`**: After route resolution, calls `debug::log_request` if route.debug != None. Before returning successful response, calls `debug::log_response` if route.debug != None.
- **`src/main.rs`**: Added `mod debug;`.
- **`src/router.rs`**, **`src/fallback.rs`**: Updated test helpers `make_route()` to include `debug: DebugLevel::default()`.
- **`config.example.toml`**: Added commented `debug = "v"` example on first route.

### Earlier logs (summarized)

## 2026-05-12 — Fallback mechanism refactoring: HealthState + probe + multi-provider error mapping

- **New `src/error_map.rs`**: Multi-provider error code classification with `ProviderType` enum (Anthropic, Deepseek, Zhipu, Kimi, OpenAi) and `ErrorClassifier` with per-provider presets. Classifies HTTP responses into Success/Retryable/NonRetryable/Fatal. Supports global config overrides.
- **New `src/probe.rs`**: Background probe loop that periodically checks unhealthy providers. Uses `tokio::select!` with timed+notify dual-wait to avoid TOCTOU race. Providers recover via probe success, with exponential backoff on probe failure.
- **`src/config.rs`**: Breaking config changes — removed `trigger_codes`, `circuit_breaker_threshold`, `circuit_breaker_cooldown_secs`. Renamed `max_retries` to `step_max_retries` (default 2). Added `provider_type` to ProviderConfig, `retryable_codes`, `retryable_error_types`, `non_retryable_codes`, `non_retryable_error_types`, `non_retryable_cooldown_secs` to FallbackConfig.
- **`src/provider.rs`**: Replaced three-state circuit breaker (Closed/Open/HalfOpen) with `HealthState` model (Healthy/Unhealthy). `FailureReason` enum distinguishes Retryable vs NonRetryable failures with different backoff strategies. `Registry` gains probe support methods: `probe_candidates()`, `min_next_probe()`, `notify_probe()`.
- **`src/fallback.rs`**: Core rewrite — `AttemptOutcome` now has 4 variants (Success/RetryableFailure/NonRetryableFailure/Fatal). Unhealthy providers skipped until probe time. Same-step retries with exponential backoff. NonRetryable failures trigger immediate provider marking. Fatal aborts entire request.
- **`src/server.rs`**: `upstream_attempt` uses `ErrorClassifier` per provider type. `read_body_string` capped at 20MB. HTTP client built once in main and shared between server and probe loop.
- **Post-review fixes**: `find_matching_rule` non-JSON body now correctly skips rules with `error_type_filter` (RED-1). `notify_probe()` added after RetryableFailure exhaustion (RED-2). `execute()` now uses fresh `epoch_secs()` per step check instead of cached `now`. Probe timeout `.max(10)` clarified with comment.
- **`src/main.rs`**: Creates `Notify`, injects into `Registry`, builds shared `HttpClient`, spawns probe loop task. `AppState.providers` is now `Arc<Registry>`.

### Earlier logs (summarized)

2026-04-25: Improved connection interruption logging levels, added Authorization Bearer header support, auto-detect HTTP vs TLS protocol on same port, fixed fallback logging model field, fixed self-signed cert persistence, adjusted log timestamps to local timezone.

2026-04-25: Added HTTPS and HTTP/2 server support with TLS module, ALPN negotiation, rcgen self-signed certs.

Pre-2026-04-25: Initial implementation with circuit breaker, fallback, auth, logging. Fixed Zhipu endpoint + TLS compatibility, added HTTP/2 to upstream client. API research and code review.
