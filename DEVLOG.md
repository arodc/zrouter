# ZRouter Development Log

## 2026-04-25 — Fix Zhipu provider (endpoint + TLS compatibility)

- Changed zhipu endpoint from OpenAI-compatible (`/api/coding/paas/v4`) to Anthropic-compatible (`/api/anthropic`)
- Fixed TLS handshake failure (`received fatal alert: ProtocolVersion`): `open.bigmodel.cn` requires TLS 1.2 but the default `hyper-rustls` connector didn't negotiate properly. Fixed by building a custom `rustls::ClientConfig` with explicit `with_protocol_versions([TLS12, TLS13])` and adding `rustls` as a direct dependency with `tls12` feature
- Added HTTP/2 support to the upstream client (`enable_http2()` on connector, `http2` features on `hyper` and `hyper-rustls`)
- Updated `docs/api-research.md` to document the Anthropic-compatible endpoint
- Both zhipu (glm-*) and deepseek (deepseek-*) providers verified working end-to-end

## 2026-04-25 — Initial implementation

- Created project skeleton with Cargo.toml and minimal dependencies (14 crates)
- Implemented TOML config parsing with provider/route/fallback configuration
- Implemented route matching with exact match, prefix wildcard, and default fallback
- Implemented provider registry with atomic circuit breaker (Closed/Open/HalfOpen)
- Implemented fallback executor with exponential backoff retry across provider steps
- Implemented Anthropic API passthrough proxy: replaces x-api-key and optional model field
- Implemented API key authentication with constant-time comparison
- Implemented structured logging (JSON/text) with request tracing (UUID v4)
- Implemented graceful shutdown via SIGTERM/SIGINT
- Added health check endpoint (GET /health)
- All 12 unit tests passing, zero compiler warnings

### Design decisions
- Pure Anthropic protocol passthrough — no format translation needed
- hyper 1.x + hyper-rustls for TLS, no heavyweight framework
- model extraction uses serde_json for correctness (replaced initial lightweight scanner)
- Circuit breaker uses SystemTime epoch seconds in AtomicU64 for lock-free concurrency
- Fallback executor passes owned AttemptParams to closures to avoid lifetime issues
- Delay resets per-step so each provider starts with initial_delay_ms

### Earlier logs (summarized)
- Conducted API research for Anthropic, DeepSeek, Zhipu AI, Kimi endpoints
- Conducted research on Anthropic-compatible endpoints (OpenRouter, CloseAI, Qiniu, laozhang, Bedrock Mantle)
- Initial plan included Anthropic↔OpenAI translation, revised to pure passthrough after scope clarification
- Code review identified 5 issues (success path missing response body, unwrap risks, auth timing leak, fragile model scanner, delay not resetting) — all fixed
