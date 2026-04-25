# ZRouter Development Log

## 2026-04-25 — Add HTTPS and HTTP/2 server support

- New `src/tls.rs` module: builds server-side TLS config from PEM cert/key files or auto-generates self-signed dev certificate via `rcgen`
- Server now uses `hyper_util::server::conn::auto::Builder` instead of `http1::Builder`, supporting both HTTP/1.1 and HTTP/2 (via ALPN `h2, http/1.1`)
- Config: added `tls`, `cert_file`, `key_file` fields to `[server]` section
- Dependencies: added `rustls-pemfile`, `rcgen`, `tokio-rustls`
- Verified: plain HTTP, HTTPS with self-signed cert, HTTP/2 negotiation

## 2026-04-25 — Fix Zhipu provider (endpoint + TLS compatibility)

- Changed zhipu endpoint from OpenAI-compatible (`/api/coding/paas/v4`) to Anthropic-compatible (`/api/anthropic`)
- Fixed TLS handshake failure (`received fatal alert: ProtocolVersion`): `open.bigmodel.cn` requires TLS 1.2 but the default `hyper-rustls` connector didn't negotiate properly. Fixed by building a custom `rustls::ClientConfig` with explicit `with_protocol_versions([TLS12, TLS13])` and adding `rustls` as a direct dependency with `tls12` feature
- Added HTTP/2 support to the upstream client (`enable_http2()` on connector, `http2` features on `hyper` and `hyper-rustls`)
- Updated `docs/api-research.md` to document the Anthropic-compatible endpoint
- Both zhipu (glm-*) and deepseek (deepseek-*) providers verified working end-to-end

## 2026-04-25 — Initial implementation

Full Anthropic API routing daemon: TOML config, route matching (exact/prefix/default), provider registry with atomic circuit breaker, fallback executor with exponential backoff, passthrough proxy, constant-time auth, structured logging, graceful shutdown. 12 unit tests, zero warnings.

### Earlier logs (summarized)
API research for Anthropic, DeepSeek, Zhipu, Kimi endpoints. Code review fixed 5 issues. Revised scope from protocol translation to pure passthrough.
