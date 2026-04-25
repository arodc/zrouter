# ZRouter Development Log

## 2026-04-25 — Fix fallback logging: wrong model field and noisy first-attempt log

- `model=None` was showing `step.model` (optional model override in route step) instead of the actual request model
- `FallbackExecutor` now receives `original_model` from the request and uses it in log messages
- "Attempting request" log removed from the happy path; only logs on retry (`attempt > 0`) or fallback (`step_idx > 0`)

## 2026-04-25 — Fix self-signed cert rejected by AI tools

- Root cause: self-signed dev cert regenerated on every restart, never persisted, untrusted by AI tool TLS stacks
- `tls.rs`: dev cert now saved to `{config_dir}/zrouter-dev-cert.pem` and reused across restarts
- Key file saved with 0600 permissions (Unix only)
- Log message shows cert file path for trust store import
- `server.rs`: post-handshake connection errors demoted from ERROR to DEBUG (client disconnects are benign noise)

## 2026-04-25 — Adjust log timestamp to local timezone, seconds precision

- Custom `LocalTimer` implementing `FormatTime` trait, format `MM/DD/HH:MM:SS`
- Compute UTC offset once at startup via `OffsetDateTime::now_local()`, cached in `OnceLock`
- Added `time` crate (already transitive dep via tracing-subscriber) with `local-offset` + `formatting` features
- Applied to both JSON and text log formats

## 2026-04-25 — Add HTTPS and HTTP/2 server support

- New `src/tls.rs` module: builds server-side TLS config from PEM cert/key files or auto-generates self-signed dev certificate via `rcgen`
- Server now uses `hyper_util::server::conn::auto::Builder` instead of `http1::Builder`, supporting both HTTP/1.1 and HTTP/2 (via ALPN `h2, http/1.1`)
- Config: added `tls`, `cert_file`, `key_file` fields to `[server]` section
- Dependencies: added `rustls-pemfile`, `rcgen`, `tokio-rustls`
- Verified: plain HTTP, HTTPS with self-signed cert, HTTP/2 negotiation

### Earlier logs (summarized)
Fixed Zhipu provider endpoint + TLS compatibility, added HTTP/2 to upstream client. Initial implementation: full routing daemon with circuit breaker, fallback, auth, logging. API research and code review.
