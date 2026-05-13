# ZRouter Development Log

## 2026-05-14 — Move hardcoded HTTP error semantics from classify() to [default] config section

- **`src/error_map.rs`**: Added `default: Option<ProviderPresetDef>` field to `ErrorCodesFile`. Updated `resolve_preset()` to fall back to `[default]` when a provider has no provider-specific section. Removed hardcoded steps 3-4 from `classify()` (non-JSON body fallback and 5xx/429/4xx default logic). `classify()` now: 2xx -> Success, preset rules -> Retryable/NonRetryable, else -> Fatal. Updated existing tests for new behavior. Added 4 new tests: `test_default_section_used_as_fallback`, `test_default_section_absent`, `test_provider_specific_overrides_default`, `test_default_section_parsed`. Total: 120 tests (all pass).
- **`error-codes.example.toml`**: Added `[default]` section before provider-specific sections with generic HTTP semantics (429/500/502/503/504 retryable, 400/401/403/404 non_retryable). Updated header comments.

## 2026-05-14 — Remove global error classification overrides from fallback config

- **`src/config.rs`**: Removed `retryable_codes`, `retryable_error_types`, `non_retryable_codes`, `non_retryable_error_types` fields from `FallbackConfig` struct and its `Default` impl.
- **`src/error_map.rs`**: Simplified `ErrorClassifier` — removed 5 global override fields, simplified `new()` to `(provider_type, error_codes)`, simplified `from_config()` to no longer take `FallbackConfig`. Removed steps 2 and 4 from `classify()` (global override code/type checks). Removed unused `HashSet` import and `crate::config::FallbackConfig` import. Removed `test_global_overrides_take_precedence` and `test_global_error_type_overrides` tests. Updated remaining test helpers and test calls. Total: 116 tests (all pass).
- **`src/server.rs`**: Updated `ErrorClassifier::from_config()` call to new signature. Removed unused `fc` clone.
- **`src/probe.rs`**: Updated `ErrorClassifier::from_config()` call to new signature.
- **`src/fallback.rs`**: Updated `make_fallback_config()` test helper to match simplified `FallbackConfig`.
- **`config.example.toml`**: Removed commented global override lines from `[fallback]` section.

## 2026-05-13 — Remove hardcoded builtin error code presets

- **`src/error_map.rs`**: Removed `builtin_preset()` function (~75 lines of hardcoded error code rules). `resolve_preset()` now returns an empty `ProviderPreset` when no `ErrorCodesFile` is provided or provider not found in file. `ErrorCodesFile::load()` changed from `Result<Option<Self>>` to `Option<Self>` — file-not-found and parse errors now log warnings and return `None` instead of fatal exit. Default classify() step 6 now treats 429 as retryable (matching universal HTTP convention). Updated all provider-specific tests to load inline TOML config instead of relying on builtin presets. Added `classifier_with_codes()` test helper. Total: 118 tests (all pass).
- **`src/main.rs`**: Simplified error codes loading — removed fatal exit on failure, load() now returns `Option<Self>` directly with warnings logged internally.

## 2026-05-13 — Externalize error code rules to TOML config file

- **`src/error_map.rs`**: Core refactor — replaced 5 static presets with owned types (`CodeRule`/`ProviderPreset` now use `String`/`Vec`). Added `ErrorCodesFile` TOML deserialization struct with `deny_unknown_fields`. Added `ProviderType::as_str()`. Added `ErrorCodesFile::load()` for file parsing. `Classification.description` changed from `Option<&'static str>` to `Option<String>`. `ErrorClassifier::from_config()` now accepts optional `ErrorCodesFile`. Added 5 new tests. Total: 123 tests.
- **`src/config.rs`**: Added `error_codes_file: Option<String>` to `Config`.
- **`src/server.rs`**: Added `error_codes` to `AppState`. Closure passes error_codes to classifier. Removed `.to_string()` conversions on description.
- **`src/main.rs`**: Loads error codes file at startup. Passes to `AppState` and `probe::run_probe_loop`.
- **`src/probe.rs`**: Added `error_codes` parameter to `run_probe_loop`. Removed `.to_string()` conversions.
- **`error-codes.example.toml`**: New file with complete 37-rule configuration for 5 providers.
- **`config.example.toml`**: Added `error_codes_file` section with commented example.

## 2026-05-13 — Enhance error code descriptions with provider-specific details

- **`docs/api-error-codes.md`**: New document listing all error codes from Anthropic, DeepSeek, Kimi (Moonshot), and Zhipu (BigModel) with cross-provider comparison table.
- **`src/error_map.rs`**: Enhanced all 5 provider presets (Anthropic/DeepSeek/Zhipu/Kimi/OpenAI) with detailed error descriptions in format `"{Type} (HTTP {code}): {cause} — {suggestion}"`. Fixed Kimi `engine_overloaded_error` from HTTP 503 to 429 (per official docs). Moved Kimi `exceeded_current_quota_error` from non_retryable 403 to retryable 429. Added Anthropic 504 timeout_error (retryable), 402 billing_error (non_retryable), 413 request_too_large (non_retryable). Added Zhipu 435 file too large (non_retryable). Added 9 new unit tests covering all changes. Total: 118 tests (all pass).

## 2026-05-12 — Update README and config for v0.3.0 debug logging

- **`README.md`**: Added "调试日志" to feature list. Added `debug.rs` row to architecture module table. Added new "调试日志" section after "日志" section with config example, output format, and three-level table.
- **`config.example.toml`**: Updated debug comment from `"v" (context length + tool names), "vv" (full content)` to `"v" (summary), "vv" (detailed)` to match README description.

### Earlier logs (summarized)

2026-05-12: Closed remaining ANSI injection surfaces in debug.rs and server.rs. Fixed ANSI sanitization, empty-body coloring, and warn-path ANSI injection. Arrow-line coloring split (header uncolored, detail UUID-colored). Thinking block truncation fix in vv-mode. Unified vv-mode response content formatting. Sub-count format edge case fixes. Inline sub-counts and direction arrows. Compact debug logging with UUID hashing and ANSI coloring. SSE parsing overhaul, color palette fixes, ANSI sanitization, UUID color.

2026-04-25: Connection interruption logging, Bearer auth, HTTP/TLS auto-detect, self-signed cert persistence, local timezone timestamps. HTTPS/HTTP/2 server with TLS module, ALPN negotiation.

Pre-2026-04-25: Initial implementation with circuit breaker, fallback, auth, logging. Fixed Zhipu endpoint + TLS compatibility, added HTTP/2 to upstream client. API research and code review.
