# API Error Codes Reference

Error codes from 4 LLM API providers: Anthropic, DeepSeek, Kimi (Moonshot), and Zhipu (BigModel).

## Anthropic

HTTP status code + `error.type` field in JSON response body.

```json
{"type": "error", "error": {"type": "not_found_error", "message": "..."}}
```

| HTTP | error.type | Description |
|------|-----------|-------------|
| 400 | `invalid_request_error` | Invalid format or content. Also used for other 4XX codes not listed |
| 401 | `authentication_error` | API key issue |
| 402 | `billing_error` | Billing or payment issue |
| 403 | `permission_error` | API key lacks permission for the resource |
| 404 | `not_found_error` | Resource not found |
| 413 | `request_too_large` | Request exceeds max size (32 MB for standard endpoints) |
| 429 | `rate_limit_error` | Rate limit hit |
| 500 | `api_error` | Internal Anthropic system error |
| 504 | `timeout_error` | Request timed out (use streaming for long requests) |
| 529 | `overloaded_error` | API temporarily overloaded |

Ref: https://docs.anthropic.com/en/api/errors

---

## DeepSeek

HTTP status code only. Compatible with OpenAI API format.

| HTTP | Description |
|------|-------------|
| 400 | Invalid Format — request body format error |
| 401 | Authentication Fails — wrong API key |
| 402 | Insufficient Balance — account out of balance |
| 422 | Invalid Parameters — request parameter error |
| 429 | Rate Limit Reached — TPM or RPM limit exceeded |
| 500 | Server Error — internal server fault |
| 503 | Server Overloaded — high traffic |

Ref: https://api-docs.deepseek.com/quick_start/error_codes

---

## Kimi (Moonshot)

HTTP status code + `error.type` field. Compatible with OpenAI API format.

```json
{"error": {"type": "invalid_request_error", "message": "..."}}
```

### 400 — Bad Request

| error.type | error.message pattern | Description |
|-----------|----------------------|-------------|
| `content_filter` | The request was rejected because it was considered high risk | Content moderation |
| `invalid_request_error` | Invalid request: {error_details} | Invalid format or missing params |
| `invalid_request_error` | Input token length too long | Exceeds context length |
| `invalid_request_error` | Your request exceeded model token limit : {max_model_length} | input + max_tokens overflow |
| `invalid_request_error` | Invalid purpose: only 'file-extract' accepted | Wrong purpose field |
| `invalid_request_error` | File size is too large, max file size is 100MB | File too large |
| `invalid_request_error` | File size is zero | Empty file upload |
| `invalid_request_error` | The number of files exceeded the max file count {max_file_count} | Too many files |

### 401 — Authentication Error

| error.type | error.message | Description |
|-----------|--------------|-------------|
| `invalid_authentication_error` | Invalid Authentication | API key incorrect |
| `incorrect_api_key_error` | Incorrect API key provided | API key missing or wrong |

Note: Keys from `platform.kimi.com` and `platform.kimi.ai` are not interchangeable.

### 403 — Permission Denied

| error.type | error.message | Description |
|-----------|--------------|-------------|
| `permission_denied_error` | The API you are accessing is not open | API not yet available |
| `permission_denied_error` | You are not allowed to get other user info | Access denied |

### 404 — Not Found

| error.type | error.message | Description |
|-----------|--------------|-------------|
| `resource_not_found_error` | Not found the model {model-id} or Permission denied | Model not found or no access |

### 429 — Rate Limit / Quota Exceeded

| error.type | error.message pattern | Description |
|-----------|----------------------|-------------|
| `engine_overloaded_error` | The engine is currently overloaded, please try again later | Concurrent limit |
| `exceeded_current_quota_error` | Your account {org-id}<{ak-id}> is suspended | Account suspended |
| `exceeded_current_quota_error` | You exceeded your current token quota | Token quota exceeded |
| `rate_limit_reached_error` | reached organization max concurrency: {Concurrency} | Concurrency limit |
| `rate_limit_reached_error` | reached organization max RPM: {RPM} | RPM limit |
| `rate_limit_reached_error` | reached organization TPM rate limit, current:{current_tpm}, limit:{max_tpm} | TPM limit |
| `rate_limit_reached_error` | reached organization TPD rate limit, current:{current_tpd}, limit:{max_tpd} | TPD limit |

### 500 — Server Error

| error.type | error.message | Description |
|-----------|--------------|-------------|
| `server_error` | Failed to extract file: {error} | File extraction failure |
| `unexpected_output` | invalid state transition | Internal error |

Ref: https://platform.kimi.ai/docs/api/errors

---

## Zhipu (BigModel)

Two-layer error model: HTTP status code (outer) + numeric business error code (inner).

```json
{"error": {"code": "1002", "message": "Authorization Token illegal..."}}
```

### HTTP Status Codes

| HTTP | Cause |
|------|-------|
| 200 | Success |
| 400 | Parameter error / abnormal file content |
| 401 | Authentication failed or Token expired |
| 429 | Concurrency exceeded / upload too fast / balance depleted / account anomaly |
| 435 | File size exceeds 100 MB |
| 500 | Server processing error |

### Business Error Codes

| Category | Code | Message |
|----------|------|---------|
| Basic | 500 | Internal Error |
| **Authentication** | | |
| Authentication | 1000 | Authentication Failed |
| Authentication | 1001 | Authentication parameter not received in Header |
| Authentication | 1002 | Invalid Authentication Token |
| Authentication | 1003 | Authentication Token expired |
| Authentication | 1004 | Authentication failed with the provided Token |
| **Account** | | |
| Account | 1100 | Account Read/Write error |
| Account | 1110 | Account currently inactive |
| Account | 1111 | Account does not exist |
| Account | 1112 | Account locked, contact customer service |
| Account | 1113 | Account in arrears, please recharge |
| Account | 1120 | Unable to access account, try again later |
| Account | 1121 | Account locked for policy violation |
| **API Call** | | |
| API Call | 1200 | API Call Error |
| API Call | 1210 | Incorrect API call parameters |
| API Call | 1211 | Model does not exist |
| API Call | 1212 | Current model does not support ${method} |
| API Call | 1213 | ${field} parameter not received |
| API Call | 1214 | Invalid ${field} parameter |
| API Call | 1215 | ${field1} and ${field2} cannot be set simultaneously |
| API Call | 1220 | No permission to access ${API_name} |
| API Call | 1221 | API ${API_name} has been taken offline |
| API Call | 1222 | API ${API_name} does not exist |
| API Call | 1230 | API call process error |
| API Call | 1231 | Duplicate request: ${request_id} |
| API Call | 1234 | Network error, id: ${error_id} |
| API Call | 1261 | Prompt too long |
| **Policy Block** | | |
| Policy Block | 1300 | API call blocked by policy |
| Policy Block | 1301 | Unsafe or sensitive content detected |
| Policy Block | 1302 | Rate limit reached (account level) |
| Policy Block | 1304 | Daily call limit reached |
| Policy Block | 1305 | Model overloaded, try again later |
| Policy Block | 1308 | Usage cap reached (${number} ${unit}), resets at ${next_flush_time} |
| Policy Block | 1309 | GLM Coding Plan subscription expired |
| Policy Block | 1310 | Weekly/monthly usage cap reached, resets at ${next_flush_time} |
| Policy Block | 1311 | Current subscription tier does not include ${model_name} |
| Policy Block | 1312 | Model overloaded, try ${model_name} instead |
| Policy Block | 1313 | Account flagged for fair-use policy violation, rate limited |

Note: In SSE streaming, errors are returned in `finish_reason` instead of error codes.

Ref: https://docs.bigmodel.cn/cn/faq/api-code

---

## Cross-Provider Comparison

| Scenario | Anthropic | DeepSeek | Kimi | Zhipu |
|----------|-----------|----------|------|-------|
| Auth failure | 401 `authentication_error` | 401 | 401 `incorrect_api_key_error` | 1000-1004 |
| Insufficient balance | 402 `billing_error` | 402 | 429 `exceeded_current_quota_error` | 1113 |
| Invalid request | 400 `invalid_request_error` | 400 / 422 | 400 `invalid_request_error` | 1210-1215 |
| Rate limit | 429 `rate_limit_error` | 429 | 429 `rate_limit_reached_error` | 1302 / 1305 / 1308 |
| Content filter | — | — | 400 `content_filter` | 1301 |
| Server error | 500 `api_error` | 500 | 500 `server_error` | 500 / HTTP 500 |
| Server overloaded | 529 `overloaded_error` | 503 | 429 `engine_overloaded_error` | 1305 / 1312 |
| Timeout | 504 `timeout_error` | — | — | — |
| Not found | 404 `not_found_error` | — | 404 `resource_not_found_error` | 1211 / 1222 |
