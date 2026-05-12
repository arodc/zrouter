# ZRouter Development Log

## 2026-05-12 — Fix system prompt truncation in vv mode

- **`src/debug.rs`**: Added `format_multiline_full(label, text)` — same multiline rendering as `format_multiline` but without `truncate_str`. System prompt text now renders in full in vv mode instead of being cut at 200 chars per paragraph. Message content still uses `format_multiline` (truncated). Added 4 tests: `format_multiline_full` (3) and integration test `test_vv_mode_system_prompt_not_truncated`. Total: 92 tests (was 88).

## 2026-05-12 — Fix tool list continuation line alignment

- **`src/debug.rs`**: Fixed `format_tool_list` continuation indent. Previously used a fixed `"tools: ".len()` (7 chars), which aligned wrapped lines after `tools:` instead of after the opening `[`. Now computes the indent from `format!("tools: {} [", count).len()`, so continuation tool names align with the first tool name on line 1. Updated `test_format_tool_list_wraps` to assert column alignment.

## 2026-05-12 — Debug output: model header, multiline content, tool wrapping, indented separator

- **`src/debug.rs`**: Four changes:
  1. **Model name header**: Replaced `DEBUG[v/vv] request/response` headers with `[{model}]` (model name in brackets). Removed `trace_id`, `model`, `debug_level` structured fields from `tracing::info!` calls — all info now in message string only, preventing trailing field dump in log output.
  2. **Multiline message content**: New `format_multiline(label, text)` helper renders newlines in message text as actual newlines with continuation lines indented to after the label's colon. Applied to user/assistant/system message content in vv-mode. Removed `content_preview` (replaced by `content_text` + `format_multiline`).
  3. **Tool list wrapping**: New `format_tool_list(names, per_line)` helper wraps tool names at 8 per line with continuation lines indented to align after `tools: `. Applied to both v-mode summary and vv-mode body. Format: `tools: {count} [Name1, Name2, ...]`.
  4. **Indented separator**: VV_SEPARATOR now uses `UUID_INDENT` prefix so the 40-dash separator aligns with content lines.
- Added 6 tests: `format_multiline` (3), `format_tool_list` (3). Total: 88 tests (was 82).

### Earlier logs (summarized)

## 2026-05-12 — Debug output: +10 indent, drop color, full tool names, no body headers, 40-dash sep

- **`src/debug.rs`**: Five changes:
  1. All content indentation increased by 10 chars (UUID_INDENT from 9→19).
  2. Removed ANSI yellow coloring on UUID (ANSI_YELLOW/ANSI_RESET constants deleted).
  3. Removed tool name truncation (`format_tool_names` deleted, `TOOL_NAMES_SHOWN` deleted); all tool names now shown via `.join(", ")`.
  4. Removed `DEBUG[vv] request body` / `DEBUG[vv] response body` header lines from vv-mode detail output; content starts directly.
  5. VV_SEPARATOR widened from 3 to 40 dashes.

- **`src/debug.rs`**: `UUID_INDENT` increased from 9 to 19 spaces. Changed ANSI yellow from `\x1b[33m` to `\u{001b}[93m` (bright yellow via Unicode escape — tracing text formatter treats `\x1b` differently). Fixed 3 `indent_after` tests for new indent width.

## 2026-05-12 — Fix ANSI colors, remove body diagnostics, clean up vv response format

- **`src/logging.rs`**: Added `.with_ansi(true)` to the compact text format builder. Without this, `tracing_subscriber` uses its default (which depends on whether stdout is a TTY), and ANSI escape codes like `\x1b[33m` were rendered as literal text in the debug output, breaking visual indentation. JSON format path unchanged.
- **`src/debug.rs`**: Removed two diagnostic log lines from `log_response` that were added during SSE parsing development:
  1. The `"response raw body | len: ..."` info log with preview/tail
  2. The `"response parse: SSE"` / `"response parse: JSON"` info log
  Kept the `"DEBUG: unable to parse response body"` warn log for actual failures.
- **`src/debug.rs`**: Simplified `format_response_body_vv` to show only content blocks, matching the request's message-by-message layout. Removed `stop_reason` and `usage` from vv body (both already appear in the v-mode summary line above). Text blocks now show plain text instead of debug-quoted format. Updated function signature (removed unused `_model`, `val`, `stop_reason`, `usage_str` params). Removed 2 diagnostic tests, updated 1 vv-format test. Total: 86 tests (was 88).

## 2026-05-12 — Suppress redundant logs when route debug is enabled

- **`src/server.rs`**: When a route has `debug` set to `V` or `Vv`, the debug output is richer than the standard request/response log lines. Wrapped three log statements in `if route.debug == DebugLevel::None` guards:
  1. `"Request received"` info log (moved after route resolution so `route.debug` is available)
  2. `"Request completed"` info log
  3. `"All providers exhausted"` warn log (fallback executor already logs detailed per-provider error info)

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
