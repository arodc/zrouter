# Anthropic API Endpoints & Routing Research

> Compiled: 2026-04-25
> Purpose: Comprehensive reference for all Anthropic API access paths -- direct, cloud-hosted (AWS Bedrock, Google Vertex AI), third-party resellers, and proxy/gateway patterns.
> Confidence: HIGH for topics 1-3 (official documentation); MEDIUM for topics 4-5 (third-party, may change).

---

## Table of Contents

1. [Official Anthropic API](#1-official-anthropic-api)
2. [Anthropic API via AWS Bedrock](#2-anthropic-api-via-aws-bedrock)
3. [Anthropic API via Google Vertex AI](#3-anthropic-api-via-google-vertex-ai)
4. [Third-Party Anthropic API Resellers & Proxies](#4-third-party-anthropic-api-resellers--proxies)
5. [API Proxy Use Cases: Key Rotation, Gateways, Routing](#5-api-proxy-use-cases-key-rotation-gateways-routing)

---

## 1. Official Anthropic API

### 1.1 Base URL & Endpoints

| Item | Value |
|------|-------|
| Base URL | `https://api.anthropic.com` |
| Messages | `POST /v1/messages` |
| Message Batches | `POST /v1/messages/batches` |
| Token Counting | `POST /v1/messages/count_tokens` |

### 1.2 Authentication

Three required headers:

```
x-api-key: $ANTHROPIC_API_KEY
anthropic-version: 2023-06-01
content-type: application/json
```

- `x-api-key`: API key from console.anthropic.com. NOT a Bearer token.
- `anthropic-version`: Required. Current stable version is `2023-06-01`.
- `content-type`: Required for JSON bodies.

Optional header for beta features:

```
anthropic-beta: <beta-flag>
```

### 1.3 Rate Limits & Usage Tiers

Rate limits are enforced via token bucket at three dimensions: RPM (requests per minute), ITPM (input tokens per minute), OTPM (output tokens per minute).

**Usage Tiers:**

| Tier | Credit Purchase Required | Monthly Spend Limit | RPM | ITPM (Haiku/Sonnet) | OTPM (Haiku/Sonnet) |
|------|-------------------------|--------------------|----|---------------------|---------------------|
| Tier 1 | $5 | $100/month | 50 | 20,000 | 4,000 |
| Tier 2 | $40 | $500/month | 50 | 50,000 | 10,000 |
| Tier 3 | $200 | $1,000/month | 50 | 50,000 | 10,000 |
| Tier 4 | $400 | $5,000/month | 50 | 50,000 | 10,000 |
| Monthly Invoicing | N/A (approval) | No limit | Higher | Higher | Higher |

Note: Opus models have lower ITPM/OTPM limits at each tier. Exact limits vary by model.

**Rate Limit Response Headers:**

```
retry-after: 30
anthropic-ratelimit-requests-limit: 50
anthropic-ratelimit-requests-remaining: 49
anthropic-ratelimit-requests-reset: 2024-01-01T00:00:00Z
anthropic-ratelimit-tokens-limit: 50000
anthropic-ratelimit-tokens-remaining: 49900
anthropic-ratelimit-tokens-reset: 2024-01-01T00:00:00Z
```

### 1.4 Request/Response Format

Identical to the Anthropic Messages API specification detailed in `api-research.md` Section 4. Key recap:

**Request:**
```json
{
  "model": "claude-sonnet-4-20250514",
  "max_tokens": 1024,
  "system": "You are a helpful assistant.",
  "messages": [{"role": "user", "content": "Hello"}],
  "stream": false
}
```

**Response:**
```json
{
  "content": [{"text": "Hi!", "type": "text"}],
  "id": "msg_013Zva2CMHLNnXjNJJKqJ2EF",
  "model": "claude-sonnet-4-20250514",
  "role": "assistant",
  "stop_reason": "end_turn",
  "type": "message",
  "usage": {"input_tokens": 25, "output_tokens": 10}
}
```

### 1.5 Important Notes

- API keys are region-agnostic; single endpoint serves all regions.
- Streaming uses named SSE events (not anonymous `data:` lines).
- `system` is a top-level parameter, not a message role.
- `content` in both request and response is an array of typed blocks.
- Extended thinking uses `thinking: {"type": "enabled", "budget_tokens": N}`.

### 1.6 Sources

- API Reference: https://docs.anthropic.com/en/api/messages
- Rate Limits: https://docs.anthropic.com/en/api/rate-limits
- Usage Tiers: https://docs.anthropic.com/en/api/usage-tiers
- Streaming: https://docs.anthropic.com/en/api/messages-streaming

---

## 2. Anthropic API via AWS Bedrock

AWS Bedrock provides **two distinct integration paths** for Claude models.

### 2.1 Path A: Legacy Bedrock API (InvokeModel / Converse)

#### Base URL & Endpoints

| Item | Value |
|------|-------|
| Base URL | `https://bedrock-runtime.{region}.amazonaws.com` |
| Invoke Model | `POST /model/{model-id}/invoke` |
| Invoke Model with Stream | `POST /model/{model-id}/invoke-with-response-stream` |
| Converse API | `POST /model/{model-id}/converse` |

Common regions: `us-east-1`, `us-west-2`, `eu-central-1`, `ap-northeast-1`

#### Model IDs

| Model | Bedrock Model ID |
|-------|-----------------|
| Claude Opus 4.6 | `anthropic.claude-opus-4-6-v1` |
| Claude Opus 4 | `anthropic.claude-opus-4-20250514-v1:0` |
| Claude Sonnet 4.5 | `anthropic.claude-sonnet-4-5-20250929-v1:0` |
| Claude Sonnet 4 | `anthropic.claude-sonnet-4-20250514-v1:0` |
| Claude Haiku 3.5 | `anthropic.claude-3-5-haiku-20241022-v1:0` |

Global endpoint model IDs use prefix `global.` (e.g., `global.anthropic.claude-opus-4-6-v1`).

#### Authentication

- **Primary**: AWS SigV4 signature (Access Key ID + Secret Access Key)
- **Alternative**: Bearer token via `AWS_BEARER_TOKEN_BEDROCK` environment variable
- **SDK**: AWS credentials resolved from environment, IAM roles, or credential files.

#### SDK Usage (Python)

```python
from anthropic import AnthropicBedrock

client = AnthropicBedrock(
    aws_access_key="AKIA...",
    aws_secret_key="...",
    aws_region="us-east-1"
)

response = client.messages.create(
    model="anthropic.claude-sonnet-4-20250514-v1:0",
    max_tokens=1024,
    messages=[{"role": "user", "content": "Hello"}]
)
```

#### Request Format

Same shape as direct Anthropic API but with `anthropic_version: "bedrock-2023-05-31"` in the request body:

```json
{
  "anthropic_version": "bedrock-2023-05-31",
  "max_tokens": 1024,
  "messages": [{"role": "user", "content": "Hello"}]
}
```

Key differences from direct API:
- `anthropic_version` is `"bedrock-2023-05-31"` (not a header, embedded in body)
- Model specified via URL path, not body parameter
- Auth is AWS SigV4, not `x-api-key` header

### 2.2 Path B: New Messages API Endpoint (Bedrock Mantle)

This is a newer, fully Anthropic-compatible endpoint on AWS infrastructure.

#### Base URL & Endpoints

| Item | Value |
|------|-------|
| Base URL | `https://bedrock-mantle.{region}.api.aws` |
| Messages Endpoint | `POST /anthropic/v1/messages` |
| Full URL Pattern | `https://bedrock-mantle.{region}.api.aws/anthropic/v1/messages` |

#### Authentication

Three auth paths supported (per platform.claude.com documentation):
1. AWS SigV4 signed requests
2. AWS Bearer tokens
3. Anthropic API key (routed through AWS infrastructure)

#### Request Format

**Identical** to the direct Anthropic Messages API. The `model` parameter goes in the request body (with the `anthropic.` provider prefix):

```json
{
  "model": "anthropic.claude-sonnet-4-20250514-v1:0",
  "max_tokens": 1024,
  "messages": [{"role": "user", "content": "Hello"}]
}
```

Use the same `anthropic-version: 2023-06-01` header as the direct API.

#### Key Advantages

- Request/response format is byte-for-byte identical to direct Anthropic API
- AWS-managed infrastructure with zero operator access (data stays in AWS)
- Can use existing Anthropic SDKs with minimal configuration changes
- Lower latency for AWS-hosted workloads

### 2.3 Comparison: Path A vs Path B

| Aspect | Legacy (InvokeModel) | New (Bedrock Mantle) |
|--------|---------------------|---------------------|
| Endpoint | `bedrock-runtime.{region}/model/{id}/invoke` | `bedrock-mantle.{region}/anthropic/v1/messages` |
| Request format | Bedrock-specific body | Identical to Anthropic API |
| Auth | AWS SigV4 / Bearer | SigV4 / Bearer / Anthropic key |
| Model in URL | Yes | No (in body) |
| SDK compat | `AnthropicBedrock` class | Standard `Anthropic` class with custom base URL |

### 2.4 Sources

- Bedrock Documentation: https://docs.anthropic.com/en/api/claude-on-amazon-bedrock
- New Mantle Endpoint: https://platform.claude.com (JS-rendered, extracted via search)
- Bedrock Model IDs: https://docs.aws.amazon.com/bedrock/

---

## 3. Anthropic API via Google Vertex AI

### 3.1 Base URL & Endpoints

Three endpoint types depending on region selection:

| Region Type | Region Value | Base URL |
|-------------|-------------|----------|
| Global | `global` | `https://aiplatform.googleapis.com` |
| Multi-region (US) | `us` | `https://aiplatform.us.rep.googleapis.com` |
| Multi-region (EU) | `eu` | `https://aiplatform.eu.rep.googleapis.com` |
| Regional | `us-east1`, `europe-west1`, etc. | `https://{region}-aiplatform.googleapis.com` |

**Full endpoint pattern:**

```
POST https://{base-url}/v1/projects/{PROJECT_ID}/locations/{REGION}/publishers/anthropic/models/{MODEL}:streamRawPredict
```

Example (global):
```
POST https://aiplatform.googleapis.com/v1/projects/my-project/locations/global/publishers/anthropic/models/claude-sonnet-4-20250514:streamRawPredict
```

### 3.2 Authentication

- **Primary**: Google Cloud service account credentials
- **Setup**: `gcloud auth application-default login` or service account key file
- **Application Default Credentials (ADC)**: Standard GCP credential chain

#### SDK Usage (Python)

```python
from anthropic import AnthropicVertex

client = AnthropicVertex(
    project_id="my-gcp-project",
    region="global"
)

response = client.messages.create(
    max_tokens=1024,
    messages=[{"role": "user", "content": "Hello"}]
)
```

### 3.3 Request Format

Two critical differences from the direct Anthropic API:

1. **Model is in the URL, NOT in the request body** -- omit the `model` field.
2. **`anthropic_version` must be `"vertex-2023-10-16"`** in the request body.

```json
{
  "anthropic_version": "vertex-2023-10-16",
  "max_tokens": 1024,
  "messages": [{"role": "user", "content": "Hello"}]
}
```

Note: No `model` field in body. No `x-api-key` header. Auth is handled by GCP credentials.

### 3.4 Response Format

Identical to the direct Anthropic Messages API response format. No transformation needed on the response side.

### 3.5 Pricing & Limits

| Aspect | Detail |
|--------|--------|
| Global endpoint | Standard Anthropic pricing |
| Multi-region / Regional | **10% premium** over standard pricing |
| Payload limit | 30 MB |
| Context window | Up to 1M tokens (Opus 4.7/4.6, Sonnet 4.6) |
| Data residency | Data stays in GCP region; no cross-region transfer |

### 3.6 Key Differences Summary

| Aspect | Direct Anthropic | Vertex AI |
|--------|-----------------|-----------|
| Model placement | In request body | In URL path |
| `anthropic_version` | Header (`2023-06-01`) | Body (`vertex-2023-10-16`) |
| Auth | `x-api-key` header | GCP ADC / service account |
| Regional pricing | N/A (single global) | 10% premium for regional |
| Infrastructure | Anthropic-hosted | GCP-hosted |

### 3.7 Sources

- Vertex AI Integration: https://docs.anthropic.com/en/api/claude-on-vertex-ai
- GCP Vertex AI Docs: https://cloud.google.com/vertex-ai/docs

---

## 4. Third-Party Anthropic API Resellers & Proxies

### 4.1 OpenRouter

OpenRouter provides two protocol modes for Claude access.

#### Anthropic-Compatible Mode ("Anthropic Skin")

| Item | Value |
|------|-------|
| Base URL | `https://openrouter.ai/api` |
| Messages Endpoint | `POST /v1/messages` |
| Full URL | `https://openrouter.ai/api/v1/messages` |

Authentication:
```
x-api-key: $OPENROUTER_API_KEY
anthropic-version: 2023-06-01
content-type: application/json
```

Request/response format: **Identical** to direct Anthropic Messages API. Drop-in replacement for `api.anthropic.com`.

Configuration for Claude Code:
```bash
export ANTHROPIC_BASE_URL=https://openrouter.ai/api
export ANTHROPIC_AUTH_TOKEN=$OPENROUTER_API_KEY
export ANTHROPIC_API_KEY=""
```

Note: `ANTHROPIC_API_KEY` must be explicitly set to empty string. `ANTHROPIC_AUTH_TOKEN` is used instead.

For fast mode support:
```bash
export CLAUDE_CODE_SKIP_FAST_MODE_ORG_CHECK=1
```

#### OpenAI-Compatible Mode

| Item | Value |
|------|-------|
| Base URL | `https://openrouter.ai/api/v1` |
| Chat Completions | `POST /chat/completions` |

Authentication:
```
Authorization: Bearer $OPENROUTER_API_KEY
```

#### Features

- Provider failover (automatic fallback between providers)
- Budget controls and spending limits
- Usage analytics dashboard
- Multiple model access through unified API

#### Gotchas

- Latency may be higher than direct API (extra hop through OpenRouter)
- Model availability depends on upstream provider status
- Pricing may differ from Anthropic direct pricing

### 4.2 Chinese Domestic Proxies

These services provide Anthropic API access from within mainland China, serving as drop-in replacements for `api.anthropic.com`.

#### CloseAI

| Item | Value |
|------|-------|
| Anthropic-native Base URL | `https://api.openai-proxy.org/anthropic` |
| Messages Endpoint | `POST /v1/messages` |
| Auth | `x-api-key: <closeai-key>` (same header format) |

Notes:
- Claims Alibaba, Tencent, Baidu as enterprise clients
- Supports both Anthropic-native and OpenAI-compatible protocols
- Request format: identical to direct Anthropic API

#### Qiniu AI (七牛云 AI)

| Item | Value |
|------|-------|
| Base URL | `https://api.qnaigc.com` |
| Anthropic-native | `POST /v1/messages` |
| OpenAI-compatible | `POST /v1/chat/completions` |
| Auth | `x-api-key: <qiniu-key>` (Anthropic mode) or `Authorization: Bearer <key>` (OpenAI mode) |

Notes:
- Cloud provider-backed (七牛云), more enterprise-oriented
- Supports both protocol modes

#### laozhang.ai

| Item | Value |
|------|-------|
| Base URL | `https://api.laozhang.ai` |
| Messages Endpoint | `POST /v1/messages` |
| Auth | `x-api-key: <laozhang-key>` |

Notes:
- Claims 99.7% API success rate
- Drop-in replacement: replace domain only, keep same request format and headers

#### POLOAPI

| Item | Value |
|------|-------|
| Base URL | `https://api.poloai.top` |
| Messages Endpoint | `POST /v1/messages` |
| Auth | `x-api-key: <polo-key>` |

Notes:
- Standard drop-in Anthropic API proxy pattern
- Same request/response format as direct Anthropic API

#### Common Pattern for Chinese Proxies

All Chinese domestic proxies follow the same integration pattern:

1. Replace `api.anthropic.com` with the proxy domain
2. Use the proxy's API key in `x-api-key` header
3. Keep request body, headers, and streaming format identical to direct Anthropic API

```bash
# Example: switch from direct to proxy
# Before: https://api.anthropic.com/v1/messages
# After:  https://api.openai-proxy.org/anthropic/v1/messages
```

#### Risk Assessment

| Risk | Mitigation |
|------|-----------|
| API key exposure to third party | Evaluate provider trustworthiness; use scoped keys |
| Service reliability | Implement fallback to direct API or multiple proxies |
| Data privacy | Review provider's data handling policies |
| Pricing transparency | Compare per-token costs with direct Anthropic pricing |
| Longevity | Have contingency plan if proxy shuts down |

### 4.3 Sources

- OpenRouter Docs: https://openrouter.ai/docs
- CloseAI: https://api.openai-proxy.org (search-extracted)
- Qiniu AI: https://api.qnaigc.com (search-extracted)
- laozhang.ai: https://api.laozhang.ai (search-extracted)
- POLOAPI: https://api.poloai.top (search-extracted)

---

## 5. API Proxy Use Cases: Key Rotation, Gateways, Routing

### 5.1 Key Rotation via Round-Robin

#### Concept

Distribute requests across multiple API keys to multiply effective rate limits. Each key has its own RPM/ITPM/OTPM quota; rotating across N keys gives ~N x quota.

#### Implementation Pattern

```python
api_keys = ["sk-ant-1...", "sk-ant-2...", "sk-ant-3..."]
key_index = 0

def get_next_key():
    global key_index
    key = api_keys[key_index % len(api_keys)]
    key_index += 1
    return key

# Per-request:
headers = {
    "x-api-key": get_next_key(),
    "anthropic-version": "2023-06-01",
    "content-type": "application/json"
}
```

#### vandamme-proxy

Open-source multi-API key round-robin rotation proxy:
- GitHub: https://github.com/nichochar/vandamme-proxy
- Configures multiple keys per provider
- Automatic round-robin distribution
- Lightweight proxy design

#### Considerations

- Rate limit headers (`anthropic-ratelimit-*`) are per-key; monitor remaining quota per key
- Keys must be in the same usage tier for predictable behavior
- Track `retry-after` header for per-key backoff

### 5.2 API Gateway Solutions

#### Kong AI Gateway

- **Type**: Enterprise-grade API gateway
- **Anthropic Support**: `llm_format: anthropic` configuration
- **Features**: Rate limiting, authentication, logging, transformations
- **Use Case**: Production deployments requiring enterprise features
- **License**: Kong Enterprise (commercial)

#### LiteLLM

- **Type**: Open-source multi-model proxy
- **Repository**: https://github.com/BerriAI/litellm
- **Anthropic Support**: Full Messages API support
- **Features**:
  - Multi-provider routing (Anthropic, OpenAI, Bedrock, Vertex, etc.)
  - Built-in key rotation across multiple keys/providers
  - Load balancing strategies (round-robin, least-latency, etc.)
  - Fallback provider chains
  - Cost tracking and budget management
  - Unified OpenAI-compatible output format
- **Deployment**: Docker, pip install, or managed cloud
- **License**: MIT (core) / commercial (enterprise features)

```yaml
# LiteLLM config example
model_list:
  - model_name: claude-sonnet
    litellm_params:
      model: anthropic/claude-sonnet-4-20250514
      api_key: sk-ant-1...
  - model_name: claude-sonnet
    litellm_params:
      model: anthropic/claude-sonnet-4-20250514
      api_key: sk-ant-2...
router_settings:
  routing_strategy: simple-shuffle
```

#### agentgateway

- **Type**: Multi-LLM load balancer
- **Repository**: https://github.com/anthropics/agentgateway
- **Features**: Agent-to-agent routing, multi-LLM load balancing
- **Note**: Anthropic's own project (experimental)

#### AWS sample-bedrock-api-proxy

- **Type**: Lightweight Bedrock conversion proxy
- **Repository**: AWS samples on GitHub
- **Purpose**: Converts Anthropic API format to Bedrock format
- **Use Case**: Transparent migration from direct Anthropic API to Bedrock

#### Alibaba Cloud AI Gateway (阿里云 AI 网关)

- **Type**: Cloud-managed API gateway service
- **Features**:
  - KMS (Key Management Service) integration for API key security
  - Rate limiting and throttling
  - Multi-provider routing
  - Audit logging
  - Cloud-native deployment
- **Use Case**: Chinese enterprise deployments requiring compliance and key management

### 5.3 Routing Patterns

#### Pattern 1: Direct Routing (Simple)

```
Client -> [Router] -> Anthropic API
```

- Single upstream, minimal latency overhead
- Suitable for simple key rotation or logging use cases

#### Pattern 2: Multi-Provider Routing

```
Client -> [Router] -> Anthropic Direct
                    -> AWS Bedrock (fallback)
                    -> Vertex AI (fallback)
```

- Route to cheapest/fastest available provider
- Failover on provider outage
- Different providers may have different rate limit pools

#### Pattern 3: Format Translation Gateway

```
Client (Anthropic format) -> [Gateway] -> DeepSeek (OpenAI format)
                                     -> Zhipu AI (OpenAI format)
                                     -> Kimi (OpenAI format)
```

- Accepts Anthropic Messages API input
- Translates to OpenAI-compatible format for Chinese providers
- Translates responses back to Anthropic format
- This is the ZRouter pattern

#### Pattern 4: Aggregated Proxy

```
Client -> [Proxy] -> Proxy Endpoint (api.proxy-domain.com)
                 -> Key rotation pool
                 -> Provider failover
                 -> Rate limit management
```

- Single endpoint with intelligent routing
- Handles key rotation, failover, rate limiting transparently
- OpenRouter and Chinese proxies implement this pattern

### 5.4 Gotchas for Proxy Implementations

1. **Streaming translation is the hardest part**: Anthropic uses named SSE events (`event: content_block_delta`), while OpenAI-compatible providers use anonymous `data:` lines. The state machine for translating streams is non-trivial.

2. **Thinking/reasoning format differs**: Anthropic uses `thinking_delta` events; Chinese providers use `reasoning_content` fields. Translation is needed.

3. **System prompt placement**: Anthropic has `system` as a top-level parameter; OpenAI-compatible APIs use `{"role": "system"}` as a message. Must extract/convert.

4. **Content block arrays**: Anthropic content is always typed blocks (`[{type: "text", text: "..."}]`). OpenAI-compatible APIs typically accept plain strings. Must handle both.

5. **Error format mapping**: Anthropic uses `{"type": "error", "error": {"type": "...", "message": "..."}}`; OpenAI-compatible uses `{"error": {"type": "...", "message": "..."}}`. Subtle but important difference.

6. **Header passthrough**: The `anthropic-version` header must be consumed by the proxy (not forwarded to OpenAI-compatible providers). The `x-api-key` must be replaced with `Authorization: Bearer <provider-key>`.

7. **Rate limit aggregation**: When routing to multiple providers, each has independent rate limits. The proxy must track limits per-provider and route accordingly.

### 5.5 Sources

- LiteLLM: https://github.com/BerriAI/litellm
- Kong AI Gateway: https://konghq.com/products/kong-ai-gateway
- agentgateway: https://github.com/anthropics/agentgateway
- vandamme-proxy: https://github.com/nichochar/vandamme-proxy
- Alibaba Cloud AI Gateway: https://www.alibabacloud.com/ (search-extracted)

---

## Appendix: Quick Reference Card

### Endpoint URL Summary

| Service | Base URL | Protocol |
|---------|----------|----------|
| Anthropic Direct | `https://api.anthropic.com` | Anthropic Messages API |
| Bedrock Legacy | `https://bedrock-runtime.{region}.amazonaws.com` | AWS InvokeModel |
| Bedrock Mantle | `https://bedrock-mantle.{region}.api.aws/anthropic` | Anthropic Messages API |
| Vertex AI (global) | `https://aiplatform.googleapis.com` | Anthropic Messages API |
| Vertex AI (US) | `https://aiplatform.us.rep.googleapis.com` | Anthropic Messages API |
| Vertex AI (EU) | `https://aiplatform.eu.rep.googleapis.com` | Anthropic Messages API |
| OpenRouter (Anthropic) | `https://openrouter.ai/api` | Anthropic Messages API |
| OpenRouter (OpenAI) | `https://openrouter.ai/api/v1` | OpenAI Chat Completions |
| CloseAI | `https://api.openai-proxy.org/anthropic` | Anthropic Messages API |
| Qiniu AI | `https://api.qnaigc.com` | Both |
| laozhang.ai | `https://api.laozhang.ai` | Anthropic Messages API |
| POLOAPI | `https://api.poloai.top` | Anthropic Messages API |

### Auth Method Summary

| Service | Header | Format |
|---------|--------|--------|
| Anthropic Direct | `x-api-key` | `x-api-key: sk-ant-...` |
| Bedrock Legacy | AWS SigV4 | Signed request |
| Bedrock Mantle | AWS SigV4 / Bearer / Anthropic key | Multiple options |
| Vertex AI | GCP ADC | Service account / OAuth |
| OpenRouter | `x-api-key` or `Authorization` | Depends on protocol mode |
| Chinese Proxies | `x-api-key` | `x-api-key: <proxy-key>` |

### Format Compatibility Summary

| Service | Request Format | Response Format | Streaming Format |
|---------|---------------|----------------|-----------------|
| Anthropic Direct | Anthropic native | Anthropic native | Named SSE events |
| Bedrock Legacy | Modified (anthropic_version in body) | Anthropic native | Named SSE events |
| Bedrock Mantle | **Identical** to direct | **Identical** to direct | **Identical** to direct |
| Vertex AI | Modified (model in URL, vertex version) | **Identical** to direct | **Identical** to direct |
| OpenRouter (Anthropic) | **Identical** to direct | **Identical** to direct | **Identical** to direct |
| OpenRouter (OpenAI) | OpenAI format | OpenAI format | Anonymous SSE |
| Chinese Proxies | **Identical** to direct | **Identical** to direct | **Identical** to direct |

---

*End of Anthropic API Endpoints & Routing Research*
