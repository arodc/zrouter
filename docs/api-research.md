# API Research for ZRouter

> Compiled: 2026-04-25
> Purpose: Routing daemon that accepts Anthropic Messages API input and routes to Chinese AI providers

---

## Table of Contents

1. [DeepSeek API](#1-deepseek-api)
2. [Zhipu AI (GLM/Z.AI) API](#2-zhipu-ai-glmzai-api)
3. [Kimi (Moonshot AI) API](#3-kimi-moonshot-ai-api)
4. [Anthropic Messages API](#4-anthropic-messages-api)
5. [Format Comparison & Routing Notes](#5-format-comparison--routing-notes)

---

## 1. DeepSeek API

### 1.1 Base URL / Endpoint

| Item | Value |
|------|-------|
| Base URL | `https://api.deepseek.com` |
| Chat Completions Endpoint | `POST /chat/completions` |
| OpenAI-compatible | Yes, same path as OpenAI |

### 1.2 Authentication

- **Method**: Bearer token in `Authorization` header
- **Header**: `Authorization: Bearer <DEEPSEEK_API_KEY>`
- **API Key Management**: `https://platform.deepseek.com/api_keys`

### 1.3 Request Format

```json
{
  "model": "deepseek-chat",
  "messages": [
    {"role": "system", "content": "You are a helpful assistant."},
    {"role": "user", "content": "Hello"}
  ],
  "max_tokens": 4096,
  "temperature": 0.7,
  "stream": false
}
```

**Key parameters**:
- `model` (string, required): Model identifier
- `messages` (array, required): Conversation messages with `role` and `content`
- `max_tokens` (integer): Maximum output tokens
- `temperature` (float): Sampling temperature (0-2)
- `stream` (boolean): Enable streaming
- `top_p`, `frequency_penalty`, `presence_penalty`: Standard OpenAI params
- `response_format`: JSON mode via `{"type": "json_object"}`
- `tools` / `tool_choice`: Function calling support

### 1.4 Response Format

```json
{
  "id": "chatcmpl-xxx",
  "object": "chat.completion",
  "created": 1700000000,
  "model": "deepseek-chat",
  "choices": [
    {
      "index": 0,
      "message": {
        "role": "assistant",
        "content": "Hello! How can I help you?"
      },
      "finish_reason": "stop"
    }
  ],
  "usage": {
    "prompt_tokens": 10,
    "completion_tokens": 20,
    "total_tokens": 30
  }
}
```

### 1.5 OpenAI Compatibility

- Fully OpenAI-compatible API
- Works with OpenAI SDK by changing `base_url`:

```python
from openai import OpenAI
client = OpenAI(api_key="your-key", base_url="https://api.deepseek.com")
```

### 1.6 Model Names

| Model | Description |
|-------|-------------|
| `deepseek-chat` | DeepSeek-V3.2 (general purpose) |
| `deepseek-reasoner` | DeepSeek-R1-0528 (reasoning/thinking) |

### 1.7 Error Codes

| Code | Name | Cause | Solution |
|------|------|-------|----------|
| 400 | Invalid Format | Invalid request body format | Fix request body per error message hints |
| 401 | Authentication Fails | Wrong API key | Check API key, create one if needed |
| 402 | Insufficient Balance | Account out of balance | Top up at platform |
| 422 | Invalid Parameters | Invalid request parameters | Fix parameters per error message hints |
| 429 | Rate Limit Reached | Sending requests too quickly | Pace requests, consider fallback providers |
| 500 | Server Error | Server-side issue | Retry after brief wait, contact support if persistent |
| 503 | Server Overloaded | High traffic overload | Retry after brief wait |

### 1.8 Streaming Support

- Enable with `"stream": true`
- Format: SSE with `data: {json}\n\n` lines
- Terminal event: `data: [DONE]`
- Standard OpenAI streaming delta format:

```
data: {"id":"chatcmpl-xxx","object":"chat.completion.chunk","created":...,"model":"deepseek-chat","choices":[{"index":0,"delta":{"content":"Hello"},"finish_reason":null}]}

data: {"id":"chatcmpl-xxx","object":"chat.completion.chunk","created":...,"model":"deepseek-chat","choices":[{"index":0,"delta":{},"finish_reason":"stop"}]}

data: [DONE]
```

### 1.9 Thinking/Reasoning Mode

- Use `deepseek-reasoner` model for reasoning
- Reasoning content delivered in `reasoning_content` field of the message/delta
- In streaming, reasoning appears before the main content

### 1.10 Sources

- API Docs: `https://api-docs.deepseek.com/`
- Error Codes: `https://api-docs.deepseek.com/quick_start/error_codes`
- Pricing: `https://api-docs.deepseek.com/quick_start/pricing`

---

## 2. Zhipu AI (GLM/Z.AI) API

### 2.1 Base URL / Endpoint

| Protocol | Region | Base URL |
|----------|--------|----------|
| OpenAI-compatible | International | `https://api.z.ai/api/paas/v4` |
| OpenAI-compatible | China (domestic) | `https://open.bigmodel.cn/api/paas/v4` |
| **Anthropic-compatible** | China (domestic) | `https://open.bigmodel.cn/api/anthropic` |

OpenAI-compatible Chat Completions Endpoint: `POST /chat/completions`
Anthropic-compatible Endpoint: `POST /v1/messages` (same path as Anthropic API)

### 2.2 Authentication

- **Method**: Bearer token in `Authorization` header
- **Header**: `Authorization: Bearer <ZHIPU_API_KEY>`
- Both international and domestic endpoints use same auth method

### 2.3 Request Format

```json
{
  "model": "glm-4.7",
  "messages": [
    {"role": "user", "content": "Hello"}
  ],
  "thinking": {
    "type": "enabled"
  },
  "max_tokens": 4096,
  "temperature": 1.0,
  "stream": false
}
```

**Key parameters**:
- `model` (string, required): Model identifier
- `messages` (array, required): Conversation messages
- `thinking` (object): `{"type": "enabled"|"disabled"}` -- enables/disables thinking mode
- `max_tokens` (integer): Maximum output tokens
- `temperature` (float): Sampling temperature
- `stream` (boolean): Enable streaming
- Standard OpenAI params also supported (`top_p`, `tools`, etc.)

### 2.4 Response Format

```json
{
  "id": "chatcmpl-xxx",
  "object": "chat.completion",
  "created": 1700000000,
  "model": "glm-4.7",
  "choices": [
    {
      "index": 0,
      "message": {
        "role": "assistant",
        "content": "Hello! How can I help you?"
      },
      "finish_reason": "stop"
    }
  ],
  "usage": {
    "prompt_tokens": 10,
    "completion_tokens": 20,
    "total_tokens": 30
  }
}
```

### 2.5 OpenAI Compatibility

- Fully OpenAI-compatible API
- Works with OpenAI SDK:

```python
from openai import OpenAI
client = OpenAI(
    api_key="your-key",
    base_url="https://api.z.ai/api/paas/v4/"
)
```

- Also has official SDK: `pip install zai-sdk` (Python), `ai.z.openapi:zai-sdk` (Java)

### 2.6 Model Names & Pricing

| Model | Context | Input/Output Price (per 1M tokens) | Cached Input |
|-------|---------|-------------------------------------|--------------|
| `glm-5` | -- | $1.00 / $1.00 | $3.20 |
| `glm-4.7` | -- | $0.60 / $0.60 | $2.20 |
| `glm-4.7-flash` | -- | Lower pricing | -- |
| `glm-4.6` | -- | $0.60 / $0.60 | $2.20 |
| `glm-4.5` | 131K | -- | -- |
| `glm-4.5-air` | 128K | -- | -- |

Note: China domestic pricing is approximately 50% lower than international pricing.

### 2.7 Error Codes

Zhipu follows standard HTTP error codes similar to OpenAI format. Error responses use the standard `{"error": {"type": "...", "message": "..."}}` format. Specific error code documentation is less detailed than DeepSeek/Kimi, but includes standard categories:

- 400: Invalid request format
- 401: Authentication failure
- 429: Rate limiting
- 500: Server error

### 2.8 Streaming Support

- Enable with `"stream": true`
- Format: SSE with `data: {json}\n\n` lines
- Terminal event: `data: [DONE]`
- Thinking mode streaming includes `reasoning_content` in delta:

```python
for chunk in response:
    if chunk.choices[0].delta.reasoning_content:
        print(chunk.choices[0].delta.reasoning_content, end="", flush=True)
    if chunk.choices[0].delta.content:
        print(chunk.choices[0].delta.content, end="", flush=True)
```

### 2.9 Thinking/Reasoning Mode

- Controlled via `thinking` parameter: `{"type": "enabled"}` or `{"type": "disabled"}`
- Default is `"enabled"` for models that support it
- Reasoning content appears in `reasoning_content` field (both streaming and non-streaming)

### 2.10 Sources

- GLM-4.7 Docs: `https://docs.z.ai/guides/llm/glm-4.7`
- Cline Provider Config: `https://docs.cline.bot/provider-config/zai`
- SDK: `https://github.com/zhipuai/zhipuai-sdk-python-v4`

---

## 3. Kimi (Moonshot AI) API

### 3.1 Base URL / Endpoint

| Item | Value |
|------|-------|
| Base URL | `https://api.moonshot.ai/v1` |
| Chat Completions Endpoint | `POST /chat/completions` |

### 3.2 Authentication

- **Method**: Bearer token in `Authorization` header
- **Header**: `Authorization: Bearer <MOONSHOT_API_KEY>`

### 3.3 Request Format

```json
{
  "model": "kimi-k2.5",
  "messages": [
    {"role": "system", "content": "You are a helpful assistant."},
    {"role": "user", "content": "Hello"}
  ],
  "max_tokens": 4096,
  "temperature": 0.7,
  "stream": false
}
```

**Key parameters**:
- `model` (string, required): Model identifier
- `messages` (array, required): Conversation messages with `role` and `content`
- `content` field supports:
  - Plain text string
  - Array of typed content blocks: `{"type": "text", "text": "..."}`, `{"type": "image_url", "image_url": {"url": "..."}}`, `{"type": "video_url", "video_url": {"url": "..."}}`
- `max_tokens` (integer): Maximum output tokens
- `max_completion_tokens` (integer): Alternative token limit
- `temperature` (float): Sampling temperature
- `stream` (boolean): Enable streaming
- `thinking` (object, kimi-k2.5 only): Thinking/reasoning mode
- `prompt_cache_key` (string): Cache key for prompt caching
- `safety_identifier` (string): Safety filtering identifier
- `response_format` (object): `{"type": "json_object"}` for JSON mode
- `stream_options` (object): `{"include_usage": true}` to include usage in streaming
- `tools` / `tool_choice`: Function calling support
- `n` (integer): Number of completions to generate

### 3.4 Response Format

```json
{
  "id": "chatcmpl-xxx",
  "object": "chat.completion",
  "created": 1700000000,
  "model": "kimi-k2.5",
  "choices": [
    {
      "index": 0,
      "message": {
        "role": "assistant",
        "content": "Hello! How can I help you?"
      },
      "finish_reason": "stop"
    }
  ],
  "usage": {
    "prompt_tokens": 10,
    "completion_tokens": 20,
    "total_tokens": 30,
    "cached_tokens": 0
  }
}
```

Note: `usage` includes a `cached_tokens` field unique to Kimi.

### 3.5 OpenAI Compatibility

- Fully OpenAI-compatible API
- Works with OpenAI SDK:

```python
from openai import OpenAI
client = OpenAI(
    api_key="your-key",
    base_url="https://api.moonshot.ai/v1"
)
```

### 3.6 Model Names

| Model | Context | Description |
|-------|---------|-------------|
| `kimi-k2.5` | -- | Latest flagship model |
| `kimi-k2-0905-preview` | -- | K2 preview (Sept 2025) |
| `kimi-k2-0711-preview` | -- | K2 preview (Jul 2025) |
| `kimi-k2-turbo-preview` | -- | K2 turbo variant |
| `kimi-k2-thinking-turbo` | -- | Thinking mode turbo |
| `kimi-k2-thinking` | -- | Thinking mode |
| `moonshot-v1-8k` | 8K | Standard model |
| `moonshot-v1-32k` | 32K | Extended context |
| `moonshot-v1-128k` | 128K | Long context |
| `moonshot-v1-auto` | Auto | Auto context length |
| `moonshot-v1-8k-vision-preview` | 8K | Vision-capable |
| `moonshot-v1-32k-vision-preview` | 32K | Vision + extended context |
| `moonshot-v1-128k-vision-preview` | 128K | Vision + long context |

### 3.7 Error Codes

Error response format:
```json
{
  "error": {
    "type": "<error_type>",
    "message": "<description>"
  }
}
```

| HTTP Status | Error Type | Description |
|-------------|------------|-------------|
| 400 | `content_filter` | Content filtered by safety |
| 400 | `invalid_request_error` (message_too_long) | Input exceeds model context |
| 400 | `invalid_request_error` (invalid_model) | Unknown model name |
| 400 | `invalid_request_error` (invalid_message) | Malformed message content |
| 400 | `invalid_request_error` (content_too_long) | Single message content exceeds limit |
| 400 | `invalid_request_error` (prompt_caching_error) | Cache key error |
| 400 | `invalid_request_error` (request_too_large) | Request body exceeds size limit |
| 400 | `invalid_request_error` (context_length_exceeded) | Context length exceeded |
| 401 | `invalid_authentication_error` | Auth header missing or format error |
| 401 | `incorrect_api_key_error` | Invalid API key |
| 403 | `exceeded_current_quota_error` | Account balance insufficient |
| 403 | `permission_denied_error` | No access to requested resource |
| 404 | `resource_not_found_error` | API path does not exist |
| 429 | `rate_limit_reached_error` (rpm) | Requests per minute limit |
| 429 | `rate_limit_reached_error` (tpm) | Tokens per minute limit |
| 429 | `rate_limit_reached_error` (tpd) | Tokens per day limit |
| 429 | `rate_limit_reached_error` (concurrency) | Concurrent request limit |
| 500 | `server_error` | Internal server error |
| 503 | `engine_overloaded_error` | Server overloaded |

### 3.8 Streaming Support

- Enable with `"stream": true`
- Format: SSE with `data: {json}\n\n` lines
- Terminal event: `data: [DONE]`
- Optional: `stream_options: {"include_usage": true}` to include token usage in final chunk

```
data: {"id":"chatcmpl-xxx","object":"chat.completion.chunk","created":...,"model":"kimi-k2.5","choices":[{"index":0,"delta":{"role":"assistant","content":""},"finish_reason":null}]}

data: {"id":"chatcmpl-xxx","object":"chat.completion.chunk","created":...,"model":"kimi-k2.5","choices":[{"index":0,"delta":{"content":"Hello"},"finish_reason":null}]}

data: {"id":"chatcmpl-xxx","object":"chat.completion.chunk","created":...,"model":"kimi-k2.5","choices":[{"index":0,"delta":{},"finish_reason":"stop"}]}

data: [DONE]
```

### 3.9 Sources

- API Reference: `https://platform.kimi.ai/docs/api/chat`
- Platform: `https://platform.moonshot.ai/`

---

## 4. Anthropic Messages API

### 4.1 Base URL / Endpoint

| Item | Value |
|------|-------|
| Base URL | `https://api.anthropic.com/v1` |
| Messages Endpoint | `POST /messages` |
| Streaming Endpoint | `POST /messages` (with `"stream": true`) |

### 4.2 Authentication

- **Method**: Custom header `x-api-key`
- **Required Headers**:

```
x-api-key: $ANTHROPIC_API_KEY
anthropic-version: 2023-06-01
content-type: application/json
```

Note: NOT Bearer token authentication. Uses `x-api-key` header directly.

### 4.3 Request Format

```json
{
  "model": "claude-sonnet-4-20250514",
  "max_tokens": 1024,
  "system": "You are a helpful assistant.",
  "messages": [
    {"role": "user", "content": "Hello"}
  ],
  "temperature": 0.7,
  "stream": false
}
```

**Key parameters**:
- `model` (string, required): Model identifier
- `messages` (array, required): Conversation messages
  - `role`: `"user"` or `"assistant"` (NOT "system")
  - `content`: string OR array of content blocks
- `max_tokens` (integer, required): Maximum output tokens (>= 1)
- `system` (string or content block array, optional): System prompt -- **top-level parameter, NOT a message role**
- `temperature` (float, optional): 0 to 1
- `stream` (boolean, optional): Enable streaming
- `thinking` (object, optional): `{"type": "enabled", "budget_tokens": N}` or `{"type": "disabled"}`
- `tools` (array, optional): Tool definitions
- `tool_choice` (object, optional): Tool selection strategy
- `stop_sequences` (array, optional): Custom stop sequences
- `top_p` (float, optional): Nucleus sampling
- `top_k` (integer, optional): Top-k sampling
- `metadata` (object, optional): `{"user_id": "..."}`
- `service_tier` (string, optional): Service tier selection

**Content block format** (when `content` is an array):

```json
{"type": "text", "text": "Hello"}
{"type": "image", "source": {"type": "base64", "media_type": "image/png", "data": "..."}}
{"type": "tool_use", "id": "...", "name": "...", "input": {...}}
{"type": "tool_result", "tool_use_id": "...", "content": "..."}
```

### 4.4 Response Format

```json
{
  "content": [
    {"text": "Hi! My name is Claude.", "type": "text"}
  ],
  "id": "msg_013Zva2CMHLNnXjNJJKqJ2EF",
  "model": "claude-sonnet-4-20250514",
  "role": "assistant",
  "stop_reason": "end_turn",
  "stop_sequence": null,
  "type": "message",
  "usage": {
    "input_tokens": 2095,
    "output_tokens": 503
  }
}
```

**Key differences from OpenAI format**:
- `content` is an array of typed content blocks, not a plain string
- `stop_reason` instead of `finish_reason`
- `role` is always `"assistant"` in response (not in choices array)
- No `choices` wrapper -- response IS the message
- `type: "message"` identifier
- `id` with `msg_` prefix (not `chatcmpl-`)

**stop_reason values**:
- `end_turn`: Natural end
- `max_tokens`: Token limit reached
- `stop_sequence`: Custom stop sequence hit
- `tool_use`: Model requesting tool call
- `pause_turn`: Extended thinking paused
- `refusal`: Model refused

**Content block types in responses**:
- `text`: Text content
- `thinking`: Extended thinking content
- `redacted_thinking`: Redacted thinking
- `tool_use`: Tool call request

### 4.5 OpenAI Compatibility

- **NOT OpenAI-compatible**. Completely different API design.
- Uses custom authentication (`x-api-key` header)
- Uses `anthropic-version` header
- `system` is a top-level parameter, not a message role
- Content is block-based (typed arrays), not plain strings
- Response structure differs significantly (no `choices` array)
- Official SDK: `@anthropic-ai/sdk` (Node), `anthropic` (Python)

### 4.6 Model Names

| Model ID | Description |
|----------|-------------|
| `claude-sonnet-4-20250514` | Claude Sonnet 4 |
| `claude-opus-4-20250514` | Claude Opus 4 |
| `claude-3-7-sonnet-20250219` | Claude 3.7 Sonnet |
| `claude-3-5-sonnet-20241022` | Claude 3.5 Sonnet (v2) |
| `claude-3-5-haiku-20241022` | Claude 3.5 Haiku |

### 4.7 Error Codes

Error response format:
```json
{
  "type": "error",
  "error": {
    "type": "<error_type>",
    "message": "<description>"
  }
}
```

| HTTP Status | Error Type | Description |
|-------------|------------|-------------|
| 400 | `invalid_request_error` | Malformed request |
| 401 | `authentication_error` | Invalid API key |
| 403 | `permission_error` | Access denied |
| 404 | `not_found_error` | Resource not found |
| 429 | `rate_limit_error` | Rate limit exceeded |
| 500 | `api_error` | Internal server error |
| 529 | `overloaded_error` | API overloaded |

### 4.8 Streaming Support

- Enable with `"stream": true`
- Format: **Named SSE events** (NOT simple `data:` lines)
- Uses `event:` and `data:` pairs

**Event types**:
- `message_start`: Initial message metadata
- `content_block_start`: New content block beginning
- `content_block_delta`: Content block incremental update
- `content_block_stop`: Content block complete
- `message_delta`: Message-level update (stop_reason, usage)
- `message_stop`: Stream complete
- `ping`: Keep-alive
- `error`: Error occurred

**Delta types** (inside `content_block_delta`):
- `text_delta`: `{"type": "text_delta", "text": "..."}`
- `thinking_delta`: `{"type": "thinking_delta", "thinking": "..."}`
- `input_json_delta`: `{"type": "input_json_delta", "partial_json": "..."}`
- `signature_delta`: `{"type": "signature_delta", "signature": "..."}`

**Stream flow**:
```
message_start
  -> [content_block_start
      -> content_block_delta (repeated)
      -> content_block_stop]*
  -> message_delta
  -> message_stop
```

**Full streaming example**:
```
event: message_start
data: {"type": "message_start", "message": {"id": "msg_...", "type": "message", "role": "assistant", "content": [], "model": "claude-sonnet-4-20250514", "stop_reason": null, "stop_sequence": null, "usage": {"input_tokens": 25, "output_tokens": 1}}}

event: content_block_start
data: {"type": "content_block_start", "index": 0, "content_block": {"type": "text", "text": ""}}

event: ping
data: {"type": "ping"}

event: content_block_delta
data: {"type": "content_block_delta", "index": 0, "delta": {"type": "text_delta", "text": "Hello"}}

event: content_block_delta
data: {"type": "content_block_delta", "index": 0, "delta": {"type": "text_delta", "text": " world"}}

event: content_block_stop
data: {"type": "content_block_stop", "index": 0}

event: message_delta
data: {"type": "message_delta", "delta": {"stop_reason": "end_turn", "stop_sequence": null}, "usage": {"output_tokens": 15}}

event: message_stop
data: {"type": "message_stop"}
```

**Error during stream**:
```
event: error
data: {"type": "error", "error": {"type": "overloaded_error", "message": "Overloaded"}}
```

### 4.9 Sources

- Messages API: `https://docs.anthropic.com/en/api/messages`
- Streaming: `https://docs.anthropic.com/en/api/messages-streaming`
- API Overview: `https://docs.anthropic.com/en/api/overview`

---

## 5. Format Comparison & Routing Notes

### 5.1 Authentication

| Provider | Header | Format |
|----------|--------|--------|
| Anthropic | `x-api-key` | `x-api-key: <key>` |
| DeepSeek | `Authorization` | `Bearer <key>` |
| Zhipu | `Authorization` | `Bearer <key>` |
| Kimi | `Authorization` | `Bearer <key>` |

### 5.2 Request Structure Differences

| Aspect | Anthropic | OpenAI-Compatible (DeepSeek/Zhipu/Kimi) |
|--------|-----------|------------------------------------------|
| System prompt | Top-level `system` param | `{"role": "system", "content": "..."}` message |
| Content format | Typed block array `[{type, text}]` or string | Plain string or typed block array |
| Max tokens | `max_tokens` (required) | `max_tokens` (optional) |
| Versioning | `anthropic-version` header | N/A |
| Thinking | `thinking: {type, budget_tokens}` | `thinking: {type}` or model-specific |
| Tool calls | In content blocks | In `tool_calls` field of message |

### 5.3 Response Structure Differences

| Aspect | Anthropic | OpenAI-Compatible |
|--------|-----------|-------------------|
| Wrapper | None (response IS the message) | `choices` array |
| Content | `content: [{type, text}]` | `message: {content: "string"}` |
| Finish reason | `stop_reason` | `finish_reason` |
| ID prefix | `msg_` | `chatcmpl-` |
| Object type | `type: "message"` | `object: "chat.completion"` |

### 5.4 Streaming Format Differences

| Aspect | Anthropic | OpenAI-Compatible |
|--------|-----------|-------------------|
| Event format | Named events: `event: <type>\ndata: <json>` | Anonymous: `data: <json>` |
| Terminal signal | `event: message_stop` | `data: [DONE]` |
| Content delivery | `content_block_delta` with typed deltas | `choices[0].delta.content` |
| Thinking delivery | `thinking_delta` events | `reasoning_content` in delta |
| Structure | Hierarchical (message -> blocks -> deltas) | Flat (one delta per chunk) |

### 5.5 Key Mapping for ZRouter Implementation

When routing from Anthropic format to an OpenAI-compatible provider:

1. **System message extraction**: Move `system` param to a `{"role": "system", "content": "..."}` message at the start of the messages array
2. **Content normalization**: If Anthropic content is a block array with a single text block, extract to plain string. If multi-modal blocks, map to provider-specific format
3. **Auth header translation**: Replace `x-api-key` with `Authorization: Bearer <provider_key>`
4. **Response wrapping**: Wrap provider response in Anthropic-style response:
   - Extract `choices[0].message.content` -> `content: [{type: "text", text: "..."}]`
   - Map `finish_reason` -> `stop_reason` (`"stop"` -> `"end_turn"`, `"length"` -> `"max_tokens"`)
   - Wrap in `{"type": "message", "role": "assistant", ...}`
5. **Streaming translation**:
   - Convert `data: {chunk}` to `event: content_block_delta\ndata: {type: "content_block_delta", ...}`
   - Wrap stream start in `message_start` and `content_block_start` events
   - Convert `data: [DONE]` to `message_delta` + `message_stop` events
6. **Error mapping**: Map provider error codes to Anthropic error types

### 5.6 Provider Selection Matrix

| Feature | DeepSeek | Zhipu | Kimi |
|---------|----------|-------|------|
| Thinking/reasoning | deepseek-reasoner model | thinking param | kimi-k2.5 thinking |
| Vision | No | Yes (some models) | Yes (vision-preview models) |
| Video | No | No | Yes (video_url content) |
| Prompt caching | Yes | Yes | Yes (with cache_key) |
| JSON mode | Yes | Yes | Yes |
| Tool calling | Yes | Yes | Yes |
| Max context | 128K | 131K | 128K |
| Domestic CN endpoint | No | Yes (open.bigmodel.cn) | No |
| OpenAI SDK compat | Yes | Yes | Yes |

---

*End of API Research Document*
