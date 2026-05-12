# ZRouter Development Log

## 2026-05-12 — Close remaining ANSI injection surfaces

- **`src/debug.rs`**: Made `sanitize_ansi` public so it can be used from server.rs. Applied `sanitize_ansi` to the header line in `log_colored_detail`, preventing ANSI injection via user-controlled model names in the uncolored header path. The single-line (no-detail) path also now sanitizes.
- **`src/server.rs`**: Applied `crate::debug::sanitize_ansi(&model)` to all three debug=none log lines (`req >>>`, `ack <<<`, `err <<<`), closing the last ANSI injection surface where user-controlled model values reached the terminal unfiltered. Total: 109 tests (all pass).

## 2026-05-12 — Fix ANSI sanitization, empty-body coloring, and warn-path ANSI injection

- **`src/logging.rs`**: Restored `.with_ansi_sanitization(false)` on the text-mode tracing subscriber. Without this setting, tracing-subscriber strips all ANSI escape codes from logged output, defeating the `colorize_uuid` coloring applied by `log_colored_detail`.
- **`src/debug.rs`**: Changed `log_response` empty-body path to use `log_colored_detail` instead of `tracing::info!`, so detail lines receive UUID-derived ANSI coloring consistent with `log_response_parsed`. Also fixed the direction arrow from `ack [` to `ack <<< [` for consistency.
- **`src/debug.rs`**: Applied `sanitize_ansi()` to the `model` parameter in both `log_request` and `log_response` warn paths, preventing ANSI injection via user-controlled model names when `with_ansi_sanitization(false)` allows escape codes through. Total: 109 tests (all pass).

## 2026-05-12 — Arrow-line coloring split: header uncolored, detail lines colored

- **`src/debug.rs`**: Added ANSI coloring infrastructure (`colorize_uuid`, `sanitize_ansi`, `uuid_color_prefix`, `COLOR_PALETTE`). Introduced `log_colored_detail` helper that splits a multi-line message at the first newline: the header line (containing direction arrows `>>>`/`<<<`) is logged uncolored, and all subsequent detail lines are wrapped in UUID-derived ANSI color. Changed `log_request` and `log_response_parsed` to use `log_colored_detail` instead of `tracing::info!`. Added 8 tests for ANSI sanitization, UUID color, and colorize_uuid behavior. Total: 109 tests (unchanged count, 6 new color tests + 2 split tests).
- **`src/server.rs`**: Removed `colorize_uuid` wrapper from the three `debug=none` log lines (`req >>>`, `ack <<<`, `err <<<`). These single-line logs now output plain text without ANSI coloring.

## 2026-05-12 — Fix thinking block truncation in vv-mode response debug

- **`src/debug.rs`**: Changed `format_response_body_vv` to use `format_multiline_full` for thinking blocks instead of `format_multiline`. The former does not truncate lines to 200 characters, matching the request-side strategy where system prompts use `format_multiline_full` (no truncation) while message content uses `format_multiline` (truncated). Text blocks in response continue to use `format_multiline`. Added 2 tests: `test_vv_mode_thinking_block_not_truncated`, `test_vv_mode_text_block_still_truncated`. Total: 109 tests (was 107).

## 2026-05-12 — Unify vv-mode response content formatting with request side

- **`src/debug.rs`**: Changed `format_response_body_vv` to use `format_multiline(&label, text)` for `text` and `thinking` content blocks instead of inline `format!` with `truncate_str`. This makes multiline response content indent continuation lines to align after the label's colon, matching the request-side formatting in `format_request_body_vv`. Added test `test_vv_mode_response_multiline_text_indented`. Total: 105 tests (was 104).

## 2026-05-12 — Fix sub-count format edge cases in debug output

- **`src/debug.rs`**: Fixed two boundary issues with inline sub-count formatting. In `log_request`, when `msg_count == 0` the output is now `messages: 0` without the bracket part (was `messages: 0 [user: 0, assistant: 0, tool_result: 0]`). In `log_response_parsed`, when `block_count > 0` but all blocks are unknown types (parts vector is empty), the output is now `content_blocks: N` without empty brackets (was `content_blocks: N []`). Added 2 tests: `test_log_request_zero_messages_no_brackets`, `test_log_response_unknown_block_types_no_empty_brackets`. Total: 104 tests (was 102).

## 2026-05-12 — Inline sub-counts and add direction arrows in debug output

- **`src/debug.rs`**: Changed `log_request` messages sub-counts from multi-line indented format to inline bracket format: `messages: N [user: a, assistant: b, tool_result: c]`. Similarly changed `log_response_parsed` content_blocks sub-counts to inline: `content_blocks: N [text: a, thinking: b, tool_use: c [Tool1, Tool2]]`. Added `>>>` direction arrow after `req` and `<<<` after `ack` to match server.rs debug=none style. The `indent_after` function is retained for `max_tokens` and `context_size` lines. All 102 tests pass without changes.

### Earlier logs (summarized)

2026-05-12: Compact debug logging with UUID hashing and ANSI coloring. SSE parsing overhaul, color palette fixes, ANSI sanitization, UUID color. Thinking block support in v/vv response debug. vv-mode multiline content indentation aligned to label colon. Inline sub-counts and direction arrows.

2026-04-25: Connection interruption logging, Bearer auth, HTTP/TLS auto-detect, self-signed cert persistence, local timezone timestamps. HTTPS/HTTP/2 server with TLS module, ALPN negotiation.

Pre-2026-04-25: Initial implementation with circuit breaker, fallback, auth, logging. Fixed Zhipu endpoint + TLS compatibility, added HTTP/2 to upstream client. API research and code review.
