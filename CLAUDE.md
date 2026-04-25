# ZRouter - Anthropic API Routing Daemon

## Overview
Rust daemon that accepts Anthropic Messages API requests and routes them to Anthropic-compatible backends based on model name. Supports fallback chains and circuit breaker protection.

## Build & Run
```bash
cargo build                    # debug build
cargo build --release          # release build (stripped, LTO, size-optimized)
cargo test                     # run all tests
cargo run -- --config config.example.toml   # run with config
```

## Architecture
- **Entry**: `src/main.rs` — daemon lifecycle, signal handling
- **Config**: `src/config.rs` — TOML parsing, validation
- **Server**: `src/server.rs` — hyper HTTP server, request dispatch
- **Proxy**: `src/proxy.rs` — model extraction/replacement from JSON body
- **Router**: `src/router.rs` — route matching (exact > prefix wildcard > default)
- **Provider**: `src/provider.rs` — provider registry, atomic circuit breaker
- **Fallback**: `src/fallback.rs` — retry loop with exponential backoff
- **Auth**: `src/auth.rs` — API key verification (constant-time)
- **Logging**: `src/logging.rs` — structured JSON/text logging

## Key Constraints
- **Warnings = build failure** (`[lints.rust] warnings = "deny"`)
- Pure Anthropic protocol passthrough, no format translation
- Minimal dependencies (14 crates)
- Circuit breaker: Closed → Open → HalfOpen state machine
- Streaming: Phase 1 (pre-stream) can fallback, Phase 2 (in-stream) cannot

## Config
See `config.example.toml` for full configuration reference.

## v1 Scope
- Supported: text requests, streaming passthrough, multi-provider routing, fallback, circuit breaker
- Not supported: non-Anthropic providers (DeepSeek/Zhipu/Kimi), AWS Bedrock Legacy, Vertex AI, multi-key round-robin

## Files
- `docs/api-research.md` — OpenAI-compatible API research (for future translation)
- `docs/anthropic-endpoints-research.md` — Anthropic endpoint compatibility reference
- `contrib/zrouter.service` — systemd unit file
- `DEVLOG.md` — development log
