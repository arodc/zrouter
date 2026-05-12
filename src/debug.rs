use std::fmt::Display;

use crate::config::DebugLevel;

const TEXT_PREVIEW_LEN: usize = 200;

/// Separator between v-mode summary and vv-mode detail in a merged log line.
/// Indented with UUID_INDENT to align with content.
const VV_SEPARATOR_DASHES: &str = "----------------------------------------";

/// Indent width: UUID first dash position (9) + 10 extra alignment = 19.
const UUID_INDENT: &str = "                   ";

/// Format a child line indented to align after the parent label's value.
/// Prepends the UUID base indent (9 spaces) so all content lines start at
/// the same column. The child indent width is `parent_label.len() + 1`
/// (label includes the colon, +1 for the space after it).
/// Example: `indent_after("messages:", "user:", &2)`
/// → `"          user: 2"` (9 base + 10 child = 19 spaces + "user: 2")
fn indent_after(parent_label: &str, child: &str, value: &dyn Display) -> String {
    let child_indent = parent_label.len() + 1;
    format!(
        "{}{} {} {}",
        UUID_INDENT,
        " ".repeat(child_indent),
        child,
        value
    )
}

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
                "{} req [{}]: unable to parse request body: {}",
                trace_id, model, e
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
        "{uuid} req [{model}]\n\
         {UUID_INDENT}messages: {msg_count}\n\
         {user_line}\n\
         {assistant_line}\n\
         {tool_result_line}\n\
         {UUID_INDENT}system: {system_line}\n\
         {tools_line}\n\
         {max_tokens_line}\n\
         {context_size_line}",
        uuid = trace_id,
        model = model,
        msg_count = msg_count,
        user_line = indent_after("messages:", "user:", &user_count),
        assistant_line = indent_after("messages:", "assistant:", &assistant_count),
        tool_result_line = indent_after("messages:", "tool_result:", &tool_result_count),
        system_line = system_line,
        tools_line = format_tool_list(&tool_names, 8),
        max_tokens_line = indent_after("tools:", "max_tokens:", &max_tokens),
        context_size_line = indent_after("tools:", "context_size:", &format!("~{} chars", content_chars)),
    );

    // vv mode: append detailed body to same message
    let msg = if level == &DebugLevel::Vv {
        let detail = format_request_body_vv(trace_id, model, &val, &tool_names);
        format!("{}\n{}{}\n{}", msg, UUID_INDENT, VV_SEPARATOR_DASHES, detail)
    } else {
        msg
    };

    tracing::info!("{}", msg);
}

/// Log debug information about an Anthropic Messages API response.
/// Never panics — JSON parse failures are logged as warnings.
pub fn log_response(trace_id: &uuid::Uuid, model: &str, level: &DebugLevel, body: &str) {
    if level == &DebugLevel::None {
        return;
    }

    if body.is_empty() || body.trim().is_empty() {
        tracing::info!(
            "{trace_id} ack [{model}]\n\
             {UUID_INDENT}stop_reason: empty\n\
             {UUID_INDENT}content_blocks: 0\n\
             {UUID_INDENT}usage: not available",
            trace_id = trace_id,
            model = model,
        );
        return;
    }

    let val: serde_json::Value = match serde_json::from_str(body) {
        Ok(v) => v,
        Err(json_err) => {
            // Try SSE: body may contain multiple `data: {...}` lines with
            // optional `event: ...` lines. We merge ALL events to reconstruct
            // stop_reason, content blocks, and usage from the full stream.
            if body.contains("data: ") || body.contains("data:") {
                if let Some((v, _event_count)) = merge_sse_events(body) {
                    log_response_parsed(trace_id, model, level, &v);
                    return;
                }
            }
            // Final fallback: warn with body preview and specific error
            tracing::warn!(
                "{} ack [{}]: unable to parse response body ({} bytes): {}",
                trace_id, model, body.len(), json_err
            );
            return;
        }
    };

    log_response_parsed(trace_id, model, level, &val);
}

/// Merge all SSE `data:` events into a synthetic response object.
///
/// Anthropic streaming spreads response data across multiple events:
/// - `message_start` → usage (input_tokens)
/// - `content_block_start/delta/stop` → content blocks (text, thinking)
/// - `message_delta` → stop_reason, usage (output_tokens)
/// - `message_stop` → no useful data
///
/// Returns `(merged_value, event_count)` or `None` if no valid data events found.
fn merge_sse_events(body: &str) -> Option<(serde_json::Value, usize)> {
    let mut stop_reason = "unknown".to_string();
    let mut input_tokens: Option<u64> = None;
    let mut output_tokens: Option<u64> = None;
    let mut content_blocks: Vec<serde_json::Value> = Vec::new();
    let mut current_block_type: Option<String> = None;
    let mut current_block_text = String::new();

    let mut event_count: usize = 0;

    for line in body.lines() {
        let json_str = match line.strip_prefix("data: ") {
            Some(s) => s,
            None => match line.strip_prefix("data:") {
                Some(s) => s,
                None => continue,
            },
        };

        let event: serde_json::Value = match serde_json::from_str(json_str) {
            Ok(v) => v,
            Err(_) => continue,
        };

        event_count += 1;

        match event["type"].as_str() {
            Some("message_start") => {
                if let Some(u) = event["message"]["usage"].as_object() {
                    input_tokens = u.get("input_tokens").and_then(|v| v.as_u64());
                }
            }
            Some("content_block_start") => {
                let cb = &event["content_block"];
                current_block_type = cb["type"].as_str().map(|s| s.to_string());
                current_block_text = cb["text"].as_str().unwrap_or("").to_string();
            }
            Some("content_block_delta") => {
                let delta = &event["delta"];
                if delta["type"] == "thinking_delta" {
                    if let Some(t) = delta["thinking"].as_str() {
                        current_block_text.push_str(t);
                    }
                } else if delta["type"] == "text_delta" {
                    if let Some(t) = delta["text"].as_str() {
                        current_block_text.push_str(t);
                    }
                }
            }
            Some("content_block_stop") => {
                if let Some(ref bt) = current_block_type {
                    let block = match bt.as_str() {
                        "text" | "thinking" => {
                            serde_json::json!({"type": bt, "text": current_block_text})
                        }
                        _ => serde_json::json!({"type": bt}),
                    };
                    content_blocks.push(block);
                }
                current_block_type = None;
                current_block_text.clear();
            }
            Some("message_delta") => {
                if let Some(sr) = event["delta"]["stop_reason"].as_str() {
                    stop_reason = sr.to_string();
                }
                if let Some(u) = event["usage"].as_object() {
                    if let Some(ot) = u.get("output_tokens").and_then(|v| v.as_u64()) {
                        output_tokens = Some(ot);
                    }
                }
            }
            _ => {}
        }
    }

    // Flush any block that was started but never closed (malformed stream)
    if let Some(ref bt) = current_block_type {
        let block = match bt.as_str() {
            "text" | "thinking" => serde_json::json!({"type": bt, "text": current_block_text}),
            _ => serde_json::json!({"type": bt}),
        };
        content_blocks.push(block);
    }

    if event_count == 0 {
        return None;
    }

    let mut usage = serde_json::Map::new();
    if let Some(i) = input_tokens {
        usage.insert("input_tokens".to_string(), serde_json::json!(i));
    }
    if let Some(o) = output_tokens {
        usage.insert("output_tokens".to_string(), serde_json::json!(o));
    }

    Some((
        serde_json::json!({
            "type": "message",
            "stop_reason": stop_reason,
            "content": content_blocks,
            "usage": usage,
        }),
        event_count,
    ))
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

    // Build v-mode summary with colon-aligned indentation
    let mut lines = vec![
        format!(
            "{trace_id} ack [{model}]",
            trace_id = trace_id,
            model = model,
        ),
        format!("{UUID_INDENT}stop_reason: {stop_reason}"),
        format!("{UUID_INDENT}content_blocks: {block_count}"),
    ];

    // Sub-counts for content blocks — align after "content_blocks:"
    if block_count > 0 {
        if text_count > 0 {
            lines.push(indent_after("content_blocks:", "text:", &text_count));
        }
        if tool_use_count > 0 {
            let names_str = tool_call_names.join(", ");
            lines.push(indent_after(
                "content_blocks:",
                "tool_use:",
                &format!("{} [{}]", tool_use_count, names_str),
            ));
        }
    }

    lines.push(format!("{UUID_INDENT}usage: {usage_str}"));

    let mut msg = lines.join("\n");

    // vv mode: append detailed content blocks to same message
    if level == &DebugLevel::Vv {
        let detail = format_response_body_vv(trace_id, content_blocks);
        msg = format!("{}\n{}{}\n{}", msg, UUID_INDENT, VV_SEPARATOR_DASHES, detail);
    }

    tracing::info!("{}", msg);
}

// ---------------------------------------------------------------------------
// vv-mode body formatting
// ---------------------------------------------------------------------------

fn format_request_body_vv(
    _trace_id: &uuid::Uuid,
    _model: &str,
    val: &serde_json::Value,
    tool_names: &[&str],
) -> String {
    let mut lines: Vec<String> = Vec::new();

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
                                "{UUID_INDENT}[{}] user (tool_result: {} blocks): {} chars",
                                i, tool_result_count, total_len
                            ));
                            continue;
                        }
                    }
                    let label = format!("{UUID_INDENT}[{}] user:", i);
                    let text = content_text(content);
                    lines.push(format_multiline(&label, &text));
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
                                    "{UUID_INDENT}[{}] assistant (tool_use: [{}])",
                                    i,
                                    tool_uses.join(", ")
                                ));
                            } else {
                                let label = format!("{UUID_INDENT}[{}] assistant:", i);
                                let mut ml = format_multiline(&label, &text_preview);
                                ml.push_str(&format!(" (tool_use: [{}])", tool_uses.join(", ")));
                                lines.push(ml);
                            }
                            continue;
                        }
                    }
                    let label = format!("{UUID_INDENT}[{}] assistant:", i);
                    let text = content_text(content);
                    lines.push(format_multiline(&label, &text));
                }
                _ => {
                    let label = format!("{UUID_INDENT}[{}] {}:", i, role);
                    let text = content_text(content);
                    lines.push(format_multiline(&label, &text));
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
            lines.push(format_multiline_full(&format!("{UUID_INDENT}system:"), &sys_text));
        }
    }

    // Tools
    if !tool_names.is_empty() {
        lines.push(format_tool_list(tool_names, 8));
    }

    // max_tokens
    if let Some(mt) = val.get("max_tokens").and_then(|t| t.as_u64()) {
        lines.push(format!("{UUID_INDENT}max_tokens: {}", mt));
    }

    lines.join("\n")
}

fn format_response_body_vv(
    _trace_id: &uuid::Uuid,
    content_blocks: Option<&Vec<serde_json::Value>>,
) -> String {
    let mut lines: Vec<String> = Vec::new();

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
                        "{UUID_INDENT}[{}] text: {}",
                        i,
                        truncate_str(text, TEXT_PREVIEW_LEN)
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
                        "{UUID_INDENT}[{}] tool_use: {} {}",
                        i, name, input
                    ));
                }
                _ => {
                    lines.push(format!("{UUID_INDENT}[{}] {}: ...", i, block_type));
                }
            }
        }
    }

    lines.join("\n")
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Format text that may contain newlines, rendering them as actual newlines
/// with continuation lines indented to align after the label's colon-space.
/// If the text has no newlines, returns `"{label} {text}"`.
/// Each line is truncated to `TEXT_PREVIEW_LEN` chars.
fn format_multiline(label: &str, text: &str) -> String {
    if !text.contains('\n') {
        return format!("{} {}", label, truncate_str(text, TEXT_PREVIEW_LEN));
    }
    let indent_width = label.len() + 1; // after colon+space
    let indent = " ".repeat(indent_width);
    let lines: Vec<&str> = text.split('\n').collect();
    let mut result = format!("{} {}", label, truncate_str(lines[0], TEXT_PREVIEW_LEN));
    for line in &lines[1..] {
        result.push_str(&format!("\n{}{}", indent, truncate_str(line, TEXT_PREVIEW_LEN)));
    }
    result
}

/// Same as `format_multiline` but without truncation.
/// Used for system prompt text which should be shown in full in vv mode.
fn format_multiline_full(label: &str, text: &str) -> String {
    if !text.contains('\n') {
        return format!("{} {}", label, text);
    }
    let indent_width = label.len() + 1;
    let indent = " ".repeat(indent_width);
    let lines: Vec<&str> = text.split('\n').collect();
    let mut result = format!("{} {}", label, lines[0]);
    for line in &lines[1..] {
        result.push_str(&format!("\n{}{}", indent, line));
    }
    result
}

/// Format a tool name list with wrapping: `per_line` names per line.
/// First line: `tools: [Name1, Name2, ...]`
/// Continuation lines indented to align with the first tool name after `[`.
fn format_tool_list(names: &[&str], per_line: usize) -> String {
    if names.is_empty() {
        return format!("{UUID_INDENT}tools: []");
    }
    let prefix = "tools: [";
    // Continuation indent = UUID_INDENT + spaces matching prefix width, so
    // tool names on wrapped lines align with the first tool name on line 1.
    let indent = format!("{}{}", UUID_INDENT, " ".repeat(prefix.len()));
    let mut result = format!("{UUID_INDENT}{}", prefix);
    for (i, name) in names.iter().enumerate() {
        if i > 0 && i % per_line == 0 {
            // Wrap to new line
            result.push_str("\n");
            result.push_str(&indent);
        } else if i > 0 {
            result.push_str(", ");
        }
        result.push_str(name);
    }
    result.push(']');
    result
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

/// Extract plain text from a content field (string or array of blocks).
/// Returns empty string if absent or non-text.
fn content_text(content: Option<&serde_json::Value>) -> String {
    match content {
        Some(serde_json::Value::String(s)) => s.clone(),
        Some(serde_json::Value::Array(arr)) => extract_text_from_blocks(arr),
        _ => String::new(),
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

    // --- indent_after tests ---

    #[test]
    fn test_indent_after_basic() {
        let result = indent_after("messages:", "user:", &2);
        // UUID_INDENT(19) + "messages:"(9) + 1 = 29 spaces, then "user: 2"
        assert_eq!(result, "                              user: 2");
    }

    #[test]
    fn test_indent_after_tools() {
        let result = indent_after("tools:", "max_tokens:", &32000);
        // UUID_INDENT(19) + "tools:"(6) + 1 = 26 spaces, then "max_tokens: 32000"
        assert_eq!(result, "                           max_tokens: 32000");
    }

    #[test]
    fn test_indent_after_short_label() {
        let result = indent_after("x:", "y:", &"hello");
        // UUID_INDENT(19) + "x:"(2) + 1 = 22 spaces, then "y: hello"
        assert_eq!(result, "                       y: hello");
    }

    // --- Existing tests ---

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
        // Single SSE data line — merge produces defaults for missing fields
        let sse = r#"data: {"type":"message","stop_reason":"end_turn","content":[],"usage":{"input_tokens":100}}"#;
        log_response(&trace_id, "test", &DebugLevel::V, sse);
    }

    #[test]
    fn test_log_response_sse_multi_event() {
        let trace_id = make_trace_id();
        // Simulate a streaming response with multiple SSE events
        let sse = "\
event: message_start\ndata: {\"type\":\"message_start\",\"message\":{\"id\":\"msg_1\",\"usage\":{\"input_tokens\":50}}}\n\n\
event: content_block_delta\ndata: {\"type\":\"content_block_delta\",\"delta\":{\"type\":\"text_delta\",\"text\":\"Hi\"}}\n\n\
event: message_delta\ndata: {\"type\":\"message_delta\",\"delta\":{\"stop_reason\":\"end_turn\"},\"usage\":{\"output_tokens\":5}}\n\n";
        log_response(&trace_id, "test", &DebugLevel::V, sse);
    }

    #[test]
    fn test_log_response_sse_with_event_lines() {
        let trace_id = make_trace_id();
        let sse = "\
event: message_start\n\
data: {\"type\":\"message_start\"}\n\
\n\
event: message_delta\n\
data: {\"type\":\"message_delta\",\"delta\":{\"stop_reason\":\"tool_use\"},\"usage\":{\"output_tokens\":42}}\n\
\n";
        log_response(&trace_id, "test", &DebugLevel::Vv, sse);
    }

    #[test]
    fn test_merge_sse_single_event() {
        let sse = r#"data: {"type":"message","stop_reason":"end_turn"}"#;
        let (v, count) = merge_sse_events(sse).unwrap();
        assert_eq!(count, 1);
        // Single event has no message_start/content_block/message_delta,
        // so defaults apply: stop_reason="unknown", content=[], usage={}
        assert_eq!(v["stop_reason"], "unknown");
    }

    #[test]
    fn test_merge_sse_no_data_lines() {
        let sse = "event: ping\n\n";
        assert!(merge_sse_events(sse).is_none());
    }

    #[test]
    fn test_merge_sse_invalid_json_ignored() {
        let sse = "data: [DONE]\n\ndata: {\"type\":\"message\"}\n\n";
        let (v, count) = merge_sse_events(sse).unwrap();
        assert_eq!(count, 1);
        assert_eq!(v["type"], "message");
    }

    #[test]
    fn test_merge_sse_full_streaming_response() {
        let sse = "\
event: message_start\n\
data: {\"type\":\"message_start\",\"message\":{\"id\":\"msg_1\",\"usage\":{\"input_tokens\":56,\"output_tokens\":0}}}\n\
\n\
event: content_block_start\n\
data: {\"type\":\"content_block_start\",\"index\":0,\"content_block\":{\"type\":\"thinking\",\"text\":\"\"}}\n\
\n\
event: content_block_delta\n\
data: {\"type\":\"content_block_delta\",\"index\":0,\"delta\":{\"type\":\"thinking_delta\",\"thinking\":\"Let me think\"}}\n\
\n\
event: content_block_stop\n\
data: {\"type\":\"content_block_stop\",\"index\":0}\n\
\n\
event: content_block_start\n\
data: {\"type\":\"content_block_start\",\"index\":1,\"content_block\":{\"type\":\"text\",\"text\":\"\"}}\n\
\n\
event: content_block_delta\n\
data: {\"type\":\"content_block_delta\",\"index\":1,\"delta\":{\"type\":\"text_delta\",\"text\":\"Hello.\"}}\n\
\n\
event: content_block_delta\n\
data: {\"type\":\"content_block_delta\",\"index\":1,\"delta\":{\"type\":\"text_delta\",\"text\":\" World.\"}}\n\
\n\
event: content_block_stop\n\
data: {\"type\":\"content_block_stop\",\"index\":1}\n\
\n\
event: message_delta\n\
data: {\"type\":\"message_delta\",\"delta\":{\"stop_reason\":\"end_turn\",\"stop_sequence\":null},\"usage\":{\"input_tokens\":56,\"output_tokens\":39}}\n\
\n\
event: message_stop\n\
data: {\"type\":\"message_stop\"}\n\
\n";

        let (v, count) = merge_sse_events(sse).unwrap();

        // 10 data lines total
        assert_eq!(count, 10);

        // stop_reason from message_delta
        assert_eq!(v["stop_reason"], "end_turn");

        // content blocks: thinking + text
        let blocks = v["content"].as_array().unwrap();
        assert_eq!(blocks.len(), 2);
        assert_eq!(blocks[0]["type"], "thinking");
        assert_eq!(blocks[0]["text"], "Let me think");
        assert_eq!(blocks[1]["type"], "text");
        assert_eq!(blocks[1]["text"], "Hello. World.");

        // usage from message_start + message_delta
        assert_eq!(v["usage"]["input_tokens"], 56);
        assert_eq!(v["usage"]["output_tokens"], 39);
    }

    #[test]
    fn test_merge_sse_tool_use_response() {
        let sse = "\
event: message_start\n\
data: {\"type\":\"message_start\",\"message\":{\"id\":\"msg_2\",\"usage\":{\"input_tokens\":100,\"output_tokens\":0}}}\n\
\n\
event: content_block_start\n\
data: {\"type\":\"content_block_start\",\"index\":0,\"content_block\":{\"type\":\"text\",\"text\":\"\"}}\n\
\n\
event: content_block_delta\n\
data: {\"type\":\"content_block_delta\",\"index\":0,\"delta\":{\"type\":\"text_delta\",\"text\":\"I'll check.\"}}\n\
\n\
event: content_block_stop\n\
data: {\"type\":\"content_block_stop\",\"index\":0}\n\
\n\
event: content_block_start\n\
data: {\"type\":\"content_block_start\",\"index\":1,\"content_block\":{\"type\":\"tool_use\",\"id\":\"tu_1\",\"name\":\"read_file\",\"input\":{}}}\n\
\n\
event: content_block_stop\n\
data: {\"type\":\"content_block_stop\",\"index\":1}\n\
\n\
event: message_delta\n\
data: {\"type\":\"message_delta\",\"delta\":{\"stop_reason\":\"tool_use\"},\"usage\":{\"output_tokens\":50}}\n\
\n";

        let (v, count) = merge_sse_events(sse).unwrap();
        assert_eq!(count, 7);
        assert_eq!(v["stop_reason"], "tool_use");

        let blocks = v["content"].as_array().unwrap();
        assert_eq!(blocks.len(), 2);
        assert_eq!(blocks[0]["type"], "text");
        assert_eq!(blocks[0]["text"], "I'll check.");
        // tool_use block has no text field, just type
        assert_eq!(blocks[1]["type"], "tool_use");

        assert_eq!(v["usage"]["input_tokens"], 100);
        assert_eq!(v["usage"]["output_tokens"], 50);
    }

    #[test]
    fn test_merge_sse_malformed_unclosed_block() {
        // Block started but never closed (stream interrupted)
        let sse = "\
data: {\"type\":\"content_block_start\",\"index\":0,\"content_block\":{\"type\":\"text\",\"text\":\"\"}}\n\
data: {\"type\":\"content_block_delta\",\"index\":0,\"delta\":{\"type\":\"text_delta\",\"text\":\"partial\"}}\n";

        let (v, count) = merge_sse_events(sse).unwrap();
        assert_eq!(count, 2);

        // Unclosed block should be flushed
        let blocks = v["content"].as_array().unwrap();
        assert_eq!(blocks.len(), 1);
        assert_eq!(blocks[0]["type"], "text");
        assert_eq!(blocks[0]["text"], "partial");
    }

    #[test]
    fn test_merge_sse_empty_body() {
        assert!(merge_sse_events("").is_none());
    }

    #[test]
    fn test_merge_sse_only_message_stop() {
        let sse = "data: {\"type\":\"message_stop\"}\n\n";
        let (v, count) = merge_sse_events(sse).unwrap();
        assert_eq!(count, 1);
        assert_eq!(v["stop_reason"], "unknown");
        assert!(v["content"].as_array().unwrap().is_empty());
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
        // "cafe" + combining acute = 5 chars, 6 bytes
        let s = "cafe\u{0301}xyz";
        let truncated = truncate_str(s, 5);
        // Should not panic on char boundary
        assert!(truncated.len() <= 5);
    }

    #[test]
    fn test_format_multiline_no_newlines() {
        let result = format_multiline("label:", "hello world");
        assert_eq!(result, "label: hello world");
    }

    #[test]
    fn test_format_multiline_with_newlines() {
        let result = format_multiline("label:", "line1\nline2\nline3");
        assert_eq!(result, "label: line1\n       line2\n       line3");
    }

    #[test]
    fn test_format_multiline_truncates_each_line() {
        let long = "a".repeat(300);
        let result = format_multiline("x:", &format!("{}\n{}", long, long));
        // Each line should be truncated to 200 chars
        for line in result.split('\n').skip(1) {
            assert!(line.trim().len() <= 200);
        }
    }

    #[test]
    fn test_format_multiline_full_no_newlines() {
        let result = format_multiline_full("label:", "hello world");
        assert_eq!(result, "label: hello world");
    }

    #[test]
    fn test_format_multiline_full_with_newlines() {
        let result = format_multiline_full("label:", "line1\nline2\nline3");
        assert_eq!(result, "label: line1\n       line2\n       line3");
    }

    #[test]
    fn test_format_multiline_full_no_truncation() {
        let long = "a".repeat(5000);
        let result = format_multiline_full("x:", &format!("{}\n{}", long, long));
        // Full text preserved — no truncation
        let lines: Vec<&str> = result.split('\n').collect();
        assert_eq!(lines.len(), 2);
        // First line: "x: " (3 chars) + 5000 = 5003
        assert_eq!(lines[0].len(), 5003);
        // Continuation line: indent (3 spaces) + 5000 = 5003
        assert_eq!(lines[1].len(), 5003);
    }

    #[test]
    fn test_vv_mode_system_prompt_not_truncated() {
        let trace_id = make_trace_id();
        let long_system = "X".repeat(5000);
        let body = serde_json::json!({
            "model": "claude-sonnet-4-20250514",
            "max_tokens": 1024,
            "system": long_system,
            "messages": [
                {"role": "user", "content": "hi"},
            ],
        });
        let body_bytes = serde_json::to_vec(&body).unwrap();
        // Should not panic — system prompt rendered in full
        log_request(&trace_id, "test", &DebugLevel::Vv, &body_bytes);
    }

    #[test]
    fn test_format_tool_list_empty() {
        let result = format_tool_list(&[] as &[&str], 8);
        assert_eq!(result, "                   tools: []");
    }

    #[test]
    fn test_format_tool_list_few() {
        let result = format_tool_list(&["Bash", "Read"], 8);
        assert!(result.contains("tools: [Bash, Read]"));
        assert!(!result.contains('\n'));
    }

    #[test]
    fn test_format_tool_list_wraps() {
        let names: Vec<&str> = vec![
            "A", "B", "C", "D", "E", "F", "G", "H", "I", "J",
        ];
        let result = format_tool_list(&names, 8);
        // First line has 8, second line has 2
        let lines: Vec<&str> = result.split('\n').collect();
        assert_eq!(lines.len(), 2);
        assert!(lines[0].contains("tools: ["));
        assert!(lines[0].contains("A, B, C, D, E, F, G, H"));
        assert!(lines[1].contains("I, J"));
        assert!(lines[1].ends_with(']'));
        // Continuation line tool names must align with first tool name on line 1.
        // "tools: 10 [" = 11 chars, so indent = UUID_INDENT(19) + 11 = 30 spaces.
        let first_tool_col = lines[0].find('A').unwrap();
        let cont_tool_col = lines[1].find('I').unwrap();
        assert_eq!(first_tool_col, cont_tool_col);
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
        let detail = format_response_body_vv(&trace_id, Some(blocks));

        assert!(detail.contains("[0] text:"));
        assert!(detail.contains("[1] tool_use: read_file"));
        assert!(detail.contains("[2] text:"));
        assert!(!detail.contains("stop_reason:"));
        assert!(!detail.contains("usage:"));
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
        assert!(detail.contains("[0] user:"));
        assert!(detail.contains("[1] assistant:"));
        assert!(detail.contains("tool_use: [read_file]"));
        assert!(detail.contains("[2] user (tool_result: 1 blocks)"));
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
