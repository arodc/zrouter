# ZRouter Development Log

## 2026-04-25 — Ensure all active connection interruptions are logged at info/warn level

- Peek timeout (30s no data): `debug` → `info`
- Connection closed (hyper error): `debug` → `info`
- Auth failure (401): no log → `warn`
- TLS handshake failure and protocol mismatch: already `warn` ✓

## 2026-04-25 — Accept Authorization Bearer header for API key auth

- Some clients (e.g. Claude Code) send API key via `Authorization: Bearer <key>` instead of `x-api-key`
- Server now checks both headers: `x-api-key` first, then `Authorization: Bearer` as fallback
- Fixes 401 authentication errors from clients that don't use `x-api-key`

## 2026-04-25 — Auto-detect HTTP vs TLS protocol on same port

- `server.rs`: use `TcpStream::peek` to read first byte without consuming it
- TLS ClientHello starts with 0x16, HTTP starts with ASCII letters
- When TLS is configured, the port now accepts both HTTPS and plain HTTP connections
- Logs `Plaintext HTTP on TLS port` at INFO level when HTTP is detected on a TLS-enabled port
- Fixes `InvalidContentType` handshake error when clients connect with `http://` to HTTPS port

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
