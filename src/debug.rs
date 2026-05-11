use crate::config::DebugLevel;

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
                                // Count tool_result content (string or array of text blocks)
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
    let system_info = if sys_len > 0 {
        content_chars += sys_len;
        format!("{} chars", sys_len)
    } else {
        "absent".to_string()
    };

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

    let level_label = if level == &DebugLevel::Vv { "vv" } else { "v" };

    tracing::info!(
        trace_id = %trace_id,
        model = model,
        level = level_label,
        msg_count,
        user_count,
        assistant_count,
        tool_result_count,
        system = %system_info,
        tool_count,
        tool_names = %tool_names_display(&tool_names),
        max_tokens = %max_tokens,
        content_chars,
        "DEBUG request",
    );

    if level == &DebugLevel::Vv {
        match serde_json::to_string_pretty(&val) {
            Ok(pretty) => {
                tracing::info!(
                    trace_id = %trace_id,
                    model = model,
                    "DEBUG[vv] request body:\n{}",
                    pretty,
                );
            }
            Err(e) => {
                tracing::warn!(
                    trace_id = %trace_id,
                    model = model,
                    error = %e,
                    "DEBUG[vv] unable to pretty-print request body"
                );
            }
        }
    }
}

/// Log debug information about an Anthropic Messages API response.
/// Never panics — JSON parse failures are logged as warnings.
pub fn log_response(trace_id: &uuid::Uuid, model: &str, level: &DebugLevel, body: &str) {
    if level == &DebugLevel::None {
        return;
    }

    if body.is_empty() || body.trim().is_empty() {
        tracing::info!(
            trace_id = %trace_id,
            model = model,
            level = "v",
            stop_reason = "empty",
            block_count = 0,
            text_count = 0,
            tool_use_count = 0,
            tool_calls = "",
            usage = "not available",
            "DEBUG response: empty body",
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
        (Some(i), Some(o)) => format!("input={} output={}", i, o),
        (Some(i), None) => format!("input={}", i),
        (None, Some(o)) => format!("output={}", o),
        (None, None) => "not available".to_string(),
    };

    let level_label = if level == &DebugLevel::Vv { "vv" } else { "v" };

    tracing::info!(
        trace_id = %trace_id,
        model = model,
        level = level_label,
        stop_reason,
        block_count,
        text_count,
        tool_use_count,
        tool_calls = %tool_call_names.join(", "),
        usage = %usage_str,
        "DEBUG response",
    );

    if level == &DebugLevel::Vv {
        match serde_json::to_string_pretty(val) {
            Ok(pretty) => {
                tracing::info!(
                    trace_id = %trace_id,
                    model = model,
                    "DEBUG[vv] response body:\n{}",
                    pretty,
                );
            }
            Err(e) => {
                tracing::warn!(
                    trace_id = %trace_id,
                    model = model,
                    error = %e,
                    "DEBUG[vv] unable to pretty-print response body"
                );
            }
        }
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

fn tool_names_display(names: &[&str]) -> String {
    if names.len() <= 6 {
        names.join(", ")
    } else {
        format!("{}, ... +{} more", names[..3].join(", "), names.len() - 3)
    }
}

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
    fn test_tool_names_display_few() {
        let names: Vec<&str> = vec!["read_file", "write_file"];
        assert_eq!(tool_names_display(&names), "read_file, write_file");
    }

    #[test]
    fn test_tool_names_display_many() {
        let names: Vec<&str> = vec!["a", "b", "c", "d", "e", "f", "g"];
        let result = tool_names_display(&names);
        assert!(result.contains("a, b, c"));
        assert!(result.contains("+4 more"));
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
}
