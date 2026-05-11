use crate::config::DebugLevel;

const TEXT_PREVIEW_LEN: usize = 200;
const TOOL_NAMES_SHOWN: usize = 6;

/// Log debug information about an Anthropic Messages API request.
/// Never panics — JSON parse failures are logged as warnings.
pub fn log_request(trace_id: &uuid::Uuid, model: &str, level: &DebugLevel, body: &[u8]) {
    if level == &DebugLevel::None {
        return;
    }

    let val: serde_json::Value = match serde_json::from_slice(body) {
        Ok(v) => v,
        Err(e) => {
            tracing::warn!(
                trace_id = %trace_id,
                model = model,
                error = %e,
                "DEBUG: unable to parse request body"
            );
            return;
        }
    };

    let level_tag = if level == &DebugLevel::Vv { "vv" } else { "v" };

    // Extract message counts by role
    let messages = val.get("messages").and_then(|m| m.as_array());
    let msg_count = messages.map_or(0, |a| a.len());

    let mut user_count: usize = 0;
    let mut assistant_count: usize = 0;
    let mut tool_result_count: usize = 0;
    let mut content_chars: usize = 0;

    if let Some(msgs) = messages {
        for msg in msgs {
            let role = msg.get("role").and_then(|r| r.as_str()).unwrap_or("");
            match role {
                "user" => {
                    user_count += 1;
                    if let Some(content) = msg.get("content").and_then(|c| c.as_array()) {
                        for block in content {
                            if block.get("type").and_then(|t| t.as_str()) == Some("tool_result") {
                                tool_result_count += 1;
                                content_chars += block_content_len(block.get("content"));
                            }
                        }
                    }
                }
                "assistant" => assistant_count += 1,
                _ => {}
            }
            content_chars += content_length(msg.get("content"));
        }
    }

    // System prompt
    let sys_len = val.get("system").map_or(0, |s| text_or_array_length(s));
    if sys_len > 0 {
        content_chars += sys_len;
    }

    // Tool definitions
    let tools = val.get("tools").and_then(|t| t.as_array());
    let tool_count = tools.map_or(0, |a| a.len());
    let tool_names: Vec<&str> = tools
        .into_iter()
        .flatten()
        .filter_map(|t| t.get("name").and_then(|n| n.as_str()))
        .collect();

    if let Some(tools_arr) = val.get("tools").and_then(|t| t.as_array()) {
        for tool in tools_arr {
            content_chars += serde_json::to_string(tool).map_or(0, |s| s.len());
        }
    }

    let max_tokens = val
        .get("max_tokens")
        .and_then(|t| t.as_u64())
        .map(|t| t.to_string())
        .unwrap_or_else(|| "not set".to_string());

    // Build v-mode summary message
    let system_line = if sys_len > 0 {
        format!("{} chars", sys_len)
    } else {
        "absent".to_string()
    };

    let msg = format!(
        "DEBUG[{level_tag}] request | model: {model} | trace_id: {trace_id}\n\
         \x20 messages: {msg_count}\n\
         \x20   user: {user_count}\n\
         \x20   assistant: {assistant_count}\n\
         \x20   tool_result: {tool_result_count}\n\
         \x20 system: {system_line}\n\
         \x20 tools: {tool_count} [{tools_display}]\n\
         \x20 max_tokens: {max_tokens}\n\
         \x20 context_size: ~{content_chars} chars",
        level_tag = level_tag,
        model = model,
        trace_id = trace_id,
        msg_count = msg_count,
        user_count = user_count,
        assistant_count = assistant_count,
        tool_result_count = tool_result_count,
        system_line = system_line,
        tool_count = tool_count,
        tools_display = format_tool_names(&tool_names),
        max_tokens = max_tokens,
        content_chars = content_chars,
    );

    tracing::info!(
        trace_id = %trace_id,
        model = model,
        debug_level = level_tag,
        message = %msg,
    );

    // vv mode: detailed body with formatted messages
    if level == &DebugLevel::Vv {
        let detail = format_request_body_vv(trace_id, model, &val, &tool_names);
        tracing::info!(
            trace_id = %trace_id,
            model = model,
            message = %detail,
        );
    }
}

/// Log debug information about an Anthropic Messages API response.
/// Never panics — JSON parse failures are logged as warnings.
pub fn log_response(trace_id: &uuid::Uuid, model: &str, level: &DebugLevel, body: &str) {
    if level == &DebugLevel::None {
        return;
    }

    if body.is_empty() || body.trim().is_empty() {
        let msg = format!(
            "DEBUG[v] response | model: {} | trace_id: {}\n\
             \x20 stop_reason: empty\n\
             \x20 content_blocks: 0\n\
             \x20 usage: not available",
            model, trace_id
        );
        tracing::info!(
            trace_id = %trace_id,
            model = model,
            debug_level = "v",
            message = %msg,
        );
        return;
    }

    let val: serde_json::Value = match serde_json::from_str(body) {
        Ok(v) => v,
        Err(e) => {
            // Try SSE: body may contain `data: {...}` lines
            if body.contains("data: ") {
                if let Some(json_start) = body.find("data: {") {
                    let json_part = &body[json_start + 6..]; // skip "data: "
                    if let Ok(v) = serde_json::from_str(json_part) {
                        log_response_parsed(trace_id, model, level, &v);
                        return;
                    }
                    // Try last complete SSE event
                    for segment in body.rsplit("data: ") {
                        if segment.trim().starts_with('{') {
                            if let Ok(v) = serde_json::from_str(segment.trim()) {
                                log_response_parsed(trace_id, model, level, &v);
                                return;
                            }
                            break;
                        }
                    }
                }
            }
            tracing::warn!(
                trace_id = %trace_id,
                model = model,
                error = %e,
                body_len = body.len(),
                body_preview = %&body[..body.len().min(80)],
                "DEBUG: unable to parse response body"
            );
            return;
        }
    };

    log_response_parsed(trace_id, model, level, &val);
}

fn log_response_parsed(
    trace_id: &uuid::Uuid,
    model: &str,
    level: &DebugLevel,
    val: &serde_json::Value,
) {
    let stop_reason = val
        .get("stop_reason")
        .and_then(|s| s.as_str())
        .unwrap_or("unknown");

    let content_blocks = val.get("content").and_then(|c| c.as_array());
    let block_count = content_blocks.map_or(0, |a| a.len());

    let mut text_count: usize = 0;
    let mut tool_use_count: usize = 0;
    let mut tool_call_names: Vec<&str> = Vec::new();

    if let Some(blocks) = content_blocks {
        for block in blocks {
            let block_type = block.get("type").and_then(|t| t.as_str()).unwrap_or("");
            match block_type {
                "text" => text_count += 1,
                "tool_use" => {
                    tool_use_count += 1;
                    if let Some(name) = block.get("name").and_then(|n| n.as_str()) {
                        tool_call_names.push(name);
                    }
                }
                _ => {}
            }
        }
    }

    let usage = val.get("usage");
    let input_tokens = usage
        .and_then(|u| u.get("input_tokens"))
        .and_then(|t| t.as_u64());
    let output_tokens = usage
        .and_then(|u| u.get("output_tokens"))
        .and_then(|t| t.as_u64());

    let usage_str = match (input_tokens, output_tokens) {
        (Some(i), Some(o)) => format!("input={}, output={}", i, o),
        (Some(i), None) => format!("input={}", i),
        (None, Some(o)) => format!("output={}", o),
        (None, None) => "not available".to_string(),
    };

    let level_tag = if level == &DebugLevel::Vv { "vv" } else { "v" };

    // Build v-mode summary
    let mut lines = vec![
        format!(
            "DEBUG[{}] response | model: {} | trace_id: {}",
            level_tag, model, trace_id
        ),
        format!("  stop_reason: {}", stop_reason),
        format!("  content_blocks: {}", block_count),
    ];

    // Sub-counts for content blocks
    if block_count > 0 {
        if text_count > 0 {
            lines.push(format!("    text: {}", text_count));
        }
        if tool_use_count > 0 {
            let names_str = format_tool_names(&tool_call_names);
            lines.push(format!("    tool_use: {} [{}]", tool_use_count, names_str));
        }
    }

    lines.push(format!("  usage: {}", usage_str));

    let msg = lines.join("\n");
    tracing::info!(
        trace_id = %trace_id,
        model = model,
        debug_level = level_tag,
        message = %msg,
    );

    // vv mode: detailed content blocks
    if level == &DebugLevel::Vv {
        let detail = format_response_body_vv(trace_id, model, val, stop_reason, content_blocks, &usage_str);
        tracing::info!(
            trace_id = %trace_id,
            model = model,
            message = %detail,
        );
    }
}

// ---------------------------------------------------------------------------
// vv-mode body formatting
// ---------------------------------------------------------------------------

fn format_request_body_vv(
    trace_id: &uuid::Uuid,
    model: &str,
    val: &serde_json::Value,
    tool_names: &[&str],
) -> String {
    let mut lines = vec![format!(
        "DEBUG[vv] request body | model: {} | trace_id: {}",
        model, trace_id
    )];

    // Messages
    if let Some(msgs) = val.get("messages").and_then(|m| m.as_array()) {
        for (i, msg) in msgs.iter().enumerate() {
            let role = msg.get("role").and_then(|r| r.as_str()).unwrap_or("unknown");
            let content = msg.get("content");

            match role {
                "user" => {
                    // Check for tool_result blocks
                    if let Some(arr) = content.and_then(|c| c.as_array()) {
                        let tool_result_count = arr
                            .iter()
                            .filter(|b| b.get("type").and_then(|t| t.as_str()) == Some("tool_result"))
                            .count();
                        if tool_result_count > 0 {
                            let total_len: usize = arr
                                .iter()
                                .filter(|b| b.get("type").and_then(|t| t.as_str()) == Some("tool_result"))
                                .map(|b| block_content_len(b.get("content")))
                                .sum();
                            lines.push(format!(
                                "  messages[{}] user (tool_result: {} blocks): {} chars",
                                i,
                                tool_result_count,
                                total_len
                            ));
                            continue;
                        }
                    }
                    let preview = content_preview(content);
                    lines.push(format!("  messages[{}] user: {}", i, preview));
                }
                "assistant" => {
                    // Check for tool_use blocks
                    if let Some(arr) = content.and_then(|c| c.as_array()) {
                        let tool_uses: Vec<&str> = arr
                            .iter()
                            .filter(|b| b.get("type").and_then(|t| t.as_str()) == Some("tool_use"))
                            .filter_map(|b| b.get("name").and_then(|n| n.as_str()))
                            .collect();
                        if !tool_uses.is_empty() {
                            let text_preview = extract_text_from_blocks(arr);
                            if text_preview.is_empty() {
                                lines.push(format!(
                                    "  messages[{}] assistant (tool_use: [{}])",
                                    i,
                                    tool_uses.join(", ")
                                ));
                            } else {
                                lines.push(format!(
                                    "  messages[{}] assistant: {} (tool_use: [{}])",
                                    i,
                                    truncate_str(&text_preview, TEXT_PREVIEW_LEN),
                                    tool_uses.join(", ")
                                ));
                            }
                            continue;
                        }
                    }
                    let preview = content_preview(content);
                    lines.push(format!("  messages[{}] assistant: {}", i, preview));
                }
                _ => {
                    let preview = content_preview(content);
                    lines.push(format!("  messages[{}] {}: {}", i, role, preview));
                }
            }
        }
    }

    // System
    if let Some(sys) = val.get("system") {
        let sys_text = match sys {
            serde_json::Value::String(s) => s.clone(),
            serde_json::Value::Array(arr) => arr
                .iter()
                .filter_map(|b| b.get("text").and_then(|t| t.as_str()))
                .collect::<Vec<_>>()
                .join("\n"),
            _ => String::new(),
        };
        if !sys_text.is_empty() {
            lines.push(format!("  system: {:?}", truncate_str(&sys_text, TEXT_PREVIEW_LEN)));
        }
    }

    // Tools
    if !tool_names.is_empty() {
        lines.push(format!(
            "  tools: [{}]",
            format_tool_names(tool_names)
        ));
    }

    // max_tokens
    if let Some(mt) = val.get("max_tokens").and_then(|t| t.as_u64()) {
        lines.push(format!("  max_tokens: {}", mt));
    }

    lines.join("\n")
}

fn format_response_body_vv(
    trace_id: &uuid::Uuid,
    model: &str,
    val: &serde_json::Value,
    stop_reason: &str,
    content_blocks: Option<&Vec<serde_json::Value>>,
    usage_str: &str,
) -> String {
    let mut lines = vec![format!(
        "DEBUG[vv] response body | model: {} | trace_id: {}",
        model, trace_id
    )];

    lines.push(format!("  stop_reason: {}", stop_reason));

    if let Some(blocks) = content_blocks {
        for (i, block) in blocks.iter().enumerate() {
            let block_type = block.get("type").and_then(|t| t.as_str()).unwrap_or("unknown");
            match block_type {
                "text" => {
                    let text = block
                        .get("text")
                        .and_then(|t| t.as_str())
                        .unwrap_or("");
                    lines.push(format!(
                        "  content[{}] text: {:?} ({} chars)",
                        i,
                        truncate_str(text, TEXT_PREVIEW_LEN),
                        text.len()
                    ));
                }
                "tool_use" => {
                    let name = block
                        .get("name")
                        .and_then(|n| n.as_str())
                        .unwrap_or("unknown");
                    let input = block
                        .get("input")
                        .map(|inp| format_compact_json(inp))
                        .unwrap_or_default();
                    lines.push(format!(
                        "  content[{}] tool_use: {} {}",
                        i, name, input
                    ));
                }
                _ => {
                    lines.push(format!("  content[{}] {}: ...", i, block_type));
                }
            }
        }
    }

    lines.push(format!("  usage: {}", usage_str));

    // Omit the unused `val` warning — val is kept for potential future field extraction
    let _ = val;

    lines.join("\n")
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Format tool names for display: up to TOOL_NAMES_SHOWN, then "... +N more".
fn format_tool_names(names: &[&str]) -> String {
    if names.len() <= TOOL_NAMES_SHOWN {
        names.join(", ")
    } else {
        let shown = &names[..TOOL_NAMES_SHOWN];
        format!("{}, ... +{} more", shown.join(", "), names.len() - TOOL_NAMES_SHOWN)
    }
}

/// Truncate string to max_len chars with "..." suffix if needed.
fn truncate_str(s: &str, max_len: usize) -> &str {
    if s.len() <= max_len {
        s
    } else {
        // Find a valid char boundary
        let mut end = max_len;
        while end > 0 && !s.is_char_boundary(end) {
            end -= 1;
        }
        // We return the truncated part; caller appends "..." in context
        &s[..end]
    }
}

/// Get a text preview from a content field (string or array).
fn content_preview(content: Option<&serde_json::Value>) -> String {
    match content {
        Some(serde_json::Value::String(s)) => {
            format!("{:?}", truncate_str(s, TEXT_PREVIEW_LEN))
        }
        Some(serde_json::Value::Array(arr)) => {
            let text = extract_text_from_blocks(arr);
            if text.is_empty() {
                "[blocks]".to_string()
            } else {
                format!("{:?}", truncate_str(&text, TEXT_PREVIEW_LEN))
            }
        }
        _ => "absent".to_string(),
    }
}

/// Extract concatenated text from an array of content blocks.
fn extract_text_from_blocks(blocks: &[serde_json::Value]) -> String {
    blocks
        .iter()
        .filter_map(|b| b.get("text").and_then(|t| t.as_str()))
        .collect::<Vec<_>>()
        .join("")
}

/// Format a JSON value compactly for inline display (tool_use input, etc.)
fn format_compact_json(val: &serde_json::Value) -> String {
    match val {
        serde_json::Value::Object(map) if !map.is_empty() => {
            let pairs: Vec<String> = map
                .iter()
                .map(|(k, v)| {
                    let v_str = match v {
                        serde_json::Value::String(s) => s.clone(),
                        other => other.to_string(),
                    };
                    format!("{}: {:?}", k, truncate_str(&v_str, 80))
                })
                .collect();
            format!("{{{}}}", pairs.join(", "))
        }
        _ => val.to_string(),
    }
}

/// Content length of a single content block's value (tool_result content, etc.)
fn block_content_len(val: Option<&serde_json::Value>) -> usize {
    match val {
        Some(serde_json::Value::String(s)) => s.len(),
        Some(serde_json::Value::Array(arr)) => arr
            .iter()
            .map(|b| {
                b.get("text")
                    .and_then(|t| t.as_str())
                    .map(|s| s.len())
                    .unwrap_or(0)
            })
            .sum(),
        _ => 0,
    }
}

/// Calculate approximate character length of a string-or-array field
/// (message content or system prompt). String: length. Array: sum of
/// "text" field lengths. Other: 0.
fn text_or_array_length(val: &serde_json::Value) -> usize {
    match val {
        serde_json::Value::String(s) => s.len(),
        serde_json::Value::Array(arr) => arr
            .iter()
            .map(|block| {
                block
                    .get("text")
                    .and_then(|t| t.as_str())
                    .map(|s| s.len())
                    .unwrap_or(0)
            })
            .sum(),
        _ => 0,
    }
}

fn content_length(content: Option<&serde_json::Value>) -> usize {
    content.map_or(0, |v| text_or_array_length(v))
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn make_trace_id() -> uuid::Uuid {
        uuid::Uuid::new_v4()
    }

    #[test]
    fn test_log_request_parses_messages_and_tools() {
        let trace_id = make_trace_id();
        let body = serde_json::json!({
            "model": "claude-sonnet-4-20250514",
            "max_tokens": 4096,
            "system": "You are a helpful assistant.",
            "messages": [
                {"role": "user", "content": "Hello"},
                {"role": "assistant", "content": "Hi there!"},
                {"role": "user", "content": [
                    {"type": "text", "text": "What files are here?"},
                ]},
                {"role": "assistant", "content": [
                    {"type": "text", "text": "Let me check."},
                    {"type": "tool_use", "id": "tu_1", "name": "list_files"},
                ]},
                {"role": "user", "content": [
                    {"type": "tool_result", "tool_use_id": "tu_1", "content": "file1.txt\nfile2.txt"},
                ]},
            ],
            "tools": [
                {"name": "list_files", "description": "List files"},
                {"name": "read_file", "description": "Read a file"},
                {"name": "search", "description": "Search files"},
            ],
        });

        let body_bytes = serde_json::to_vec(&body).unwrap();
        log_request(&trace_id, "claude-sonnet-4-20250514", &DebugLevel::V, &body_bytes);
        log_request(&trace_id, "claude-sonnet-4-20250514", &DebugLevel::Vv, &body_bytes);
    }

    #[test]
    fn test_log_request_malformed_json() {
        let trace_id = make_trace_id();
        log_request(&trace_id, "test", &DebugLevel::V, b"not json at all");
    }

    #[test]
    fn test_log_request_empty_body() {
        let trace_id = make_trace_id();
        log_request(&trace_id, "test", &DebugLevel::V, b"{}");
    }

    #[test]
    fn test_log_request_none_level_is_noop() {
        let trace_id = make_trace_id();
        log_request(&trace_id, "test", &DebugLevel::None, br#"{"model":"x","messages":[]}"#);
    }

    #[test]
    fn test_log_response_with_tool_use() {
        let trace_id = make_trace_id();
        let body = serde_json::json!({
            "type": "message",
            "id": "msg_123",
            "stop_reason": "tool_use",
            "content": [
                {"type": "text", "text": "I'll read that file for you."},
                {"type": "tool_use", "id": "tu_1", "name": "read_file", "input": {"path": "/tmp/test.txt"}},
                {"type": "text", "text": "And also search."},
            ],
            "usage": {
                "input_tokens": 28450,
                "output_tokens": 342,
            },
        });

        let body_str = serde_json::to_string(&body).unwrap();
        log_response(&trace_id, "claude-sonnet-4-20250514", &DebugLevel::V, &body_str);
        log_response(&trace_id, "claude-sonnet-4-20250514", &DebugLevel::Vv, &body_str);
    }

    #[test]
    fn test_log_response_text_only() {
        let trace_id = make_trace_id();
        let body = serde_json::json!({
            "type": "message",
            "id": "msg_456",
            "stop_reason": "end_turn",
            "content": [
                {"type": "text", "text": "Hello! How can I help?"},
            ],
            "usage": {
                "input_tokens": 100,
                "output_tokens": 20,
            },
        });
        let body_str = serde_json::to_string(&body).unwrap();
        log_response(&trace_id, "test", &DebugLevel::V, &body_str);
    }

    #[test]
    fn test_log_response_malformed_json() {
        let trace_id = make_trace_id();
        log_response(&trace_id, "test", &DebugLevel::V, "not json");
    }

    #[test]
    fn test_log_response_none_level_is_noop() {
        let trace_id = make_trace_id();
        log_response(&trace_id, "test", &DebugLevel::None, r#"{"type":"message"}"#);
    }

    #[test]
    fn test_log_response_empty_body() {
        let trace_id = make_trace_id();
        log_response(&trace_id, "test", &DebugLevel::V, "");
        log_response(&trace_id, "test", &DebugLevel::V, "   ");
    }

    #[test]
    fn test_log_response_sse_body() {
        let trace_id = make_trace_id();
        let sse = r#"data: {"type":"message","stop_reason":"end_turn","content":[],"usage":{"input_tokens":100}}"#;
        log_response(&trace_id, "test", &DebugLevel::V, sse);
    }

    #[test]
    fn test_content_length_string() {
        let val = serde_json::json!("hello world");
        assert_eq!(content_length(Some(&val)), 11);
    }

    #[test]
    fn test_content_length_array() {
        let val = serde_json::json!([
            {"type": "text", "text": "hello"},
            {"type": "text", "text": "world"},
        ]);
        assert_eq!(content_length(Some(&val)), 10);
    }

    #[test]
    fn test_content_length_none() {
        assert_eq!(content_length(None), 0);
    }

    #[test]
    fn test_text_or_array_length_string() {
        let val = serde_json::json!("system prompt");
        assert_eq!(text_or_array_length(&val), 13);
    }

    #[test]
    fn test_text_or_array_length_array() {
        let val = serde_json::json!([
            {"type": "text", "text": "part one"},
            {"type": "text", "text": "part two"},
        ]);
        assert_eq!(text_or_array_length(&val), 16);
    }

    #[test]
    fn test_block_content_len_string() {
        let val = serde_json::json!("tool result content here");
        assert_eq!(block_content_len(Some(&val)), 24);
    }

    #[test]
    fn test_block_content_len_array() {
        let val = serde_json::json!([
            {"type": "text", "text": "output line 1"},
            {"type": "text", "text": "output line 2"},
        ]);
        assert_eq!(block_content_len(Some(&val)), 26);
    }

    #[test]
    fn test_format_tool_names_few() {
        let names: Vec<&str> = vec!["read_file", "write_file"];
        assert_eq!(format_tool_names(&names), "read_file, write_file");
    }

    #[test]
    fn test_format_tool_names_exactly_six() {
        let names: Vec<&str> = vec!["a", "b", "c", "d", "e", "f"];
        assert_eq!(format_tool_names(&names), "a, b, c, d, e, f");
    }

    #[test]
    fn test_format_tool_names_seven() {
        let names: Vec<&str> = vec!["a", "b", "c", "d", "e", "f", "g"];
        let result = format_tool_names(&names);
        assert_eq!(result, "a, b, c, d, e, f, ... +1 more");
    }

    #[test]
    fn test_format_tool_names_many() {
        let names: Vec<&str> = vec!["a", "b", "c", "d", "e", "f", "g", "h", "i", "j"];
        let result = format_tool_names(&names);
        assert!(result.contains("a, b, c, d, e, f"));
        assert!(result.contains("+4 more"));
    }

    #[test]
    fn test_truncate_str_short() {
        assert_eq!(truncate_str("hello", 10), "hello");
    }

    #[test]
    fn test_truncate_str_exact() {
        assert_eq!(truncate_str("hello", 5), "hello");
    }

    #[test]
    fn test_truncate_str_long() {
        assert_eq!(truncate_str("hello world", 5), "hello");
    }

    #[test]
    fn test_truncate_str_unicode() {
        // "café" = "cafe" + combining acute = 5 chars, 6 bytes
        let s = "cafe\u{0301}xyz";
        let truncated = truncate_str(s, 5);
        // Should not panic on char boundary
        assert!(truncated.len() <= 5);
    }

    #[test]
    fn test_log_request_system_as_array() {
        let trace_id = make_trace_id();
        let body = serde_json::json!({
            "model": "claude-sonnet-4-20250514",
            "max_tokens": 1024,
            "system": [
                {"type": "text", "text": "You are helpful."},
                {"type": "text", "text": "Be concise."},
            ],
            "messages": [
                {"role": "user", "content": "hi"},
            ],
        });
        let body_bytes = serde_json::to_vec(&body).unwrap();
        log_request(&trace_id, "test", &DebugLevel::V, &body_bytes);
    }

    #[test]
    fn test_log_response_no_usage() {
        let trace_id = make_trace_id();
        let body = serde_json::json!({
            "type": "message",
            "stop_reason": "end_turn",
            "content": [
                {"type": "text", "text": "Done."},
            ],
        });
        let body_str = serde_json::to_string(&body).unwrap();
        log_response(&trace_id, "test", &DebugLevel::V, &body_str);
    }

    #[test]
    fn test_vv_mode_request_shows_tool_names_not_raw_json() {
        let trace_id = make_trace_id();
        let body = serde_json::json!({
            "model": "claude-sonnet-4-20250514",
            "max_tokens": 4096,
            "messages": [
                {"role": "user", "content": "Hello"},
            ],
            "tools": [
                {
                    "name": "Bash",
                    "description": "Run a shell command",
                    "input_schema": {"type": "object", "properties": {"command": {"type": "string"}}, "required": ["command"]}
                },
                {
                    "name": "Read",
                    "description": "Read a file",
                    "input_schema": {"type": "object", "properties": {"path": {"type": "string"}}, "required": ["path"]}
                },
            ],
        });

        // Verify vv mode logs without panicking and uses names only (no raw JSON)
        let body_bytes = serde_json::to_vec(&body).unwrap();
        log_request(&trace_id, "claude-sonnet-4-20250514", &DebugLevel::Vv, &body_bytes);

        // Verify internal formatting produces name-only output
        let tool_names: Vec<&str> = body
            .get("tools")
            .and_then(|t| t.as_array())
            .into_iter()
            .flatten()
            .filter_map(|t| t.get("name").and_then(|n| n.as_str()))
            .collect();
        let detail = format_request_body_vv(&trace_id, "claude-sonnet-4-20250514", &body, &tool_names);
        assert!(detail.contains("tools: [Bash, Read]"));
        assert!(!detail.contains("input_schema"));
        assert!(!detail.contains("description"));
    }

    #[test]
    fn test_vv_mode_response_shows_formatted_blocks() {
        let trace_id = make_trace_id();
        let val = serde_json::json!({
            "type": "message",
            "stop_reason": "tool_use",
            "content": [
                {"type": "text", "text": "I'll read that file for you."},
                {"type": "tool_use", "id": "tu_1", "name": "read_file", "input": {"path": "/tmp/test.txt"}},
                {"type": "text", "text": "Done."},
            ],
            "usage": {"input_tokens": 28450, "output_tokens": 342},
        });

        let blocks = val.get("content").and_then(|c| c.as_array()).unwrap();
        let detail = format_response_body_vv(
            &trace_id,
            "claude-sonnet-4-20250514",
            &val,
            "tool_use",
            Some(blocks),
            "input=28450, output=342",
        );

        assert!(detail.contains("content[0] text:"));
        assert!(detail.contains("content[1] tool_use: read_file"));
        assert!(detail.contains("content[2] text:"));
        assert!(!detail.contains("{\n")); // no raw JSON
    }

    #[test]
    fn test_vv_mode_user_tool_result_message() {
        let trace_id = make_trace_id();
        let body = serde_json::json!({
            "model": "claude-sonnet-4-20250514",
            "max_tokens": 1024,
            "messages": [
                {"role": "user", "content": "Check this"},
                {"role": "assistant", "content": [
                    {"type": "text", "text": "Let me look."},
                    {"type": "tool_use", "id": "tu_1", "name": "read_file"},
                ]},
                {"role": "user", "content": [
                    {"type": "tool_result", "tool_use_id": "tu_1", "content": "file contents here"},
                ]},
            ],
        });

        let tool_names: Vec<&str> = vec!["read_file"];
        let detail = format_request_body_vv(&trace_id, "test", &body, &tool_names);
        assert!(detail.contains("messages[0] user:"));
        assert!(detail.contains("messages[1] assistant:"));
        assert!(detail.contains("tool_use: [read_file]"));
        assert!(detail.contains("messages[2] user (tool_result: 1 blocks)"));
    }

    #[test]
    fn test_format_compact_json_object() {
        let val = serde_json::json!({"path": "/tmp/test.txt", "offset": 10});
        let result = format_compact_json(&val);
        assert!(result.contains("path:"));
        assert!(result.contains("offset:"));
    }

    #[test]
    fn test_format_compact_json_empty() {
        let val = serde_json::json!(null);
        let result = format_compact_json(&val);
        assert_eq!(result, "null");
    }
}
