# ZRouter Development Log

## 2026-05-12 — Update README and config for v0.3.0 debug logging

- **`README.md`**: Added "调试日志" to feature list. Added `debug.rs` row to architecture module table. Added new "调试日志" section after "日志" section with config example, output format, and three-level table.
- **`config.example.toml`**: Updated debug comment from `"v" (context length + tool names), "vv" (full content)` to `"v" (summary), "vv" (detailed)` to match README description.

## 2026-05-12 — Close remaining ANSI injection surfaces

- **`src/debug.rs`**: Made `sanitize_ansi` public so it can be used from server.rs. Applied `sanitize_ansi` to the header line in `log_colored_detail`, preventing ANSI injection via user-controlled model names in the uncolored header path. The single-line (no-detail) path also now sanitizes.
- **`src/server.rs`**: Applied `crate::debug::sanitize_ansi(&model)` to all three debug=none log lines (`req >>>`, `ack <<<`, `err <<<`), closing the last ANSI injection surface where user-controlled model values reached the terminal unfiltered. Total: 109 tests (all pass).

## 2026-05-12 — Fix ANSI sanitization, empty-body coloring, and warn-path ANSI injection

- **`src/logging.rs`**: Restored `.with_ansi_sanitization(false)` on the text-mode tracing subscriber. Without this setting, tracing-subscriber strips all ANSI escape codes from logged output, defeating the `colorize_uuid` coloring applied by `log_colored_detail`.
- **`src/debug.rs`**: Changed `log_response` empty-body path to use `log_colored_detail` instead of `tracing::info!`, so detail lines receive UUID-derived ANSI coloring consistent with `log_response_parsed`. Also fixed the direction arrow from `ack [` to `ack <<< [` for consistency.
- **`src/debug.rs`**: Applied `sanitize_ansi()` to the `model` parameter in both `log_request` and `log_response` warn paths, preventing ANSI injection via user-controlled model names when `with_ansi_sanitization(false)` allows escape codes through. Total: 109 tests (all pass).

### Earlier logs (summarized)

2026-05-12: Arrow-line coloring split (header uncolored, detail UUID-colored). Thinking block truncation fix in vv-mode. Unified vv-mode response content formatting. Sub-count format edge case fixes. Inline sub-counts and direction arrows. Compact debug logging with UUID hashing and ANSI coloring. SSE parsing overhaul, color palette fixes, ANSI sanitization, UUID color.

2026-04-25: Connection interruption logging, Bearer auth, HTTP/TLS auto-detect, self-signed cert persistence, local timezone timestamps. HTTPS/HTTP/2 server with TLS module, ALPN negotiation.

Pre-2026-04-25: Initial implementation with circuit breaker, fallback, auth, logging. Fixed Zhipu endpoint + TLS compatibility, added HTTP/2 to upstream client. API research and code review.
