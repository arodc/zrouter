# ZRouter Development Log

## 2026-05-12 — Debug output: remove redundant headers, yellow UUID, indent after dash

- **`src/debug.rs`**: Removed `model:`, `trace_id:` from all debug header lines (redundant with surrounding server logs). UUID+req/ack prefix now colored ANSI yellow. All debug content indented 9 spaces (after first '-' in UUID pattern). Updated all format string tests.

## 2026-05-12 — Fix SSE response parsing: merge all events instead of using last event

- **`src/debug.rs`**: Replaced `extract_last_sse_json` with `merge_sse_events(body) -> Option<(Value, usize)>`. The old function returned the last SSE event (always `message_stop` with no useful data). The new function iterates all `data:` lines and merges fields from `message_start` (input_tokens), `content_block_start/delta/stop` (assembled text/thinking content blocks), `message_delta` (stop_reason, output_tokens), and `message_stop` (skipped). Returns a synthetic message object plus event count. Also flushes unclosed blocks for interrupted streams.
- Updated `log_response` to call `merge_sse_events` and log `"parsed as SSE (merged N events)"`.
- Removed `extract_last_sse_json` entirely.
- Replaced 5 old tests with 7 new tests: `test_merge_sse_single_event`, `test_merge_sse_no_data_lines`, `test_merge_sse_invalid_json_ignored`, `test_merge_sse_full_streaming_response`, `test_merge_sse_tool_use_response`, `test_merge_sse_malformed_unclosed_block`, `test_merge_sse_empty_body`, `test_merge_sse_only_message_stop`. Updated `test_log_response_sse_multi_event` with delta type fields. Total: 88 tests (was 85).

## 2026-05-12 — Restore log level, colon-aligned indent, response diagnostics

- **`src/logging.rs`**: Restored log level display in compact text format. Changed `with_level(false)` to `with_level(true)`. Format is now `HH:MM:SS LEVEL message` (e.g. `12:34:56 INFO Request received`). JSON format unchanged.
- **`src/debug.rs`**: Three changes:
  1. **Colon-aligned indentation**: New `indent_after(parent_label, child, value)` helper computes indent as `parent_label.len() + 1`. Children of `messages:` (9 chars) indent 10 spaces; children of `tools:` (6 chars) indent 7 spaces. Applied to all v-mode and v-mode request/response output: message sub-counts (user/assistant/tool_result), tool sub-fields (max_tokens/context_size), content_blocks sub-counts (text/tool_use).
  2. **Response raw body diagnostics**: `log_response` now emits an INFO log before parsing with body length, first 500 chars preview, and last 500 chars tail. After parsing, logs which path was taken: "parsed as JSON", "parsed as SSE (last event)", "empty body", or "parse failed" (with json_error and has_data_prefix fields).
  3. **vv-mode tool_result fix**: Tool result messages in vv-mode now use consistent `"  messages[N] user (tool_result: X blocks): Y chars"` format.
- Tests added: `test_indent_after_basic`, `test_indent_after_tools`, `test_indent_after_short_label`, `test_log_response_raw_body_diagnostic_json`, `test_log_response_raw_body_diagnostic_sse`. Total: 85 tests (was 80).

### Earlier logs (summarized)

2026-05-12: Debug log format overhaul -- compact output (`HH:MM:SS message`), merged v/vv mode into single tracing call per event, SSE response parsing with `extract_last_sse_json`, message direction prefix. Human-readable formatted output with label:value tag style. Vv mode trims tool definitions to names only. Debug logging refinements: context_size counts tool_result content, empty body detection, structured log fields. Startup version info from git. Per-model debug logging: `DebugLevel` enum, `log_request`/`log_response` in new `src/debug.rs`.

2026-05-12: Fallback mechanism refactoring -- `HealthState` model replacing circuit breaker, `ErrorClassifier` for multi-provider error codes, background probe loop with `tokio::select!`, `AttemptOutcome` 4-variant enum, provider-type-aware config.

2026-04-25: Connection interruption logging, Bearer auth, HTTP/TLS auto-detect, self-signed cert persistence, local timezone timestamps. HTTPS/HTTP/2 server with TLS module, ALPN negotiation.

Pre-2026-04-25: Initial implementation with circuit breaker, fallback, auth, logging. Fixed Zhipu endpoint + TLS compatibility, added HTTP/2 to upstream client. API research and code review.
