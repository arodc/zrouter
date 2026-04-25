# ZRouter

Anthropic API 路由守护进程。根据模型名称将请求分发到多个 Anthropic 兼容后端，支持回退链、熔断器和 HTTPS。

## 功能

- **模型路由**：精确匹配 > 前缀通配符（`claude-opus-*`）> 默认路由
- **多级回退**：每个路由配置多个 provider，自动重试和切换
- **熔断器**：连续失败自动跳过 provider，冷却后探测恢复
- **指数退避**：可配置的重试延迟
- **TLS/HTTPS**：自定义证书或自动生成持久化自签名证书，支持 HTTP/1.1 + HTTP/2
- **API Key 认证**：可选的路由级认证，constant-time 比较防止时序攻击
- **结构化日志**：JSON 或 text 格式，本地时区时间戳
- **健康检查**：`GET /health`
- **优雅关闭**：SIGINT / SIGTERM

## 构建

```bash
cargo build                        # debug
cargo build --release              # release（stripped, LTO, size 优化）
cargo test                         # 运行测试
```

需要 Rust 1.75+。release 构建警告视为错误。

## 运行

```bash
cargo run -- --config config.example.toml
./target/release/zrouter --config /etc/zrouter/config.toml
```

## 配置

完整示例见 [`config.example.toml`](config.example.toml)。

### 服务端

```toml
[server]
bind = "127.0.0.1"
port = 3827
api_key = "sk-zrouter-secret"     # 可选，或用 ZROUTER_API_KEY 环境变量
max_body_size = 10485760           # 10MB

# TLS（可选）
tls = true
cert_file = "/path/to/cert.pem"   # 省略则自动生成自签名开发证书
key_file = "/path/to/key.pem"     # 持久化到 {config_dir}/zrouter-dev-*.pem
```

### Provider

```toml
[providers.anthropic]
endpoint = "https://api.anthropic.com"
api_key_env = "ANTHROPIC_API_KEY"  # 环境变量名
# api_key = "sk-..."              # 或直接写（优先级高于 env）
connect_timeout_secs = 10
read_timeout_secs = 300
```

### 路由

```toml
[[route]]
model = "claude-opus-*"
steps = [
  { provider = "anthropic" },
  { provider = "openrouter", model = "anthropic/claude-opus-4" },  # model 可选，用于替换模型名
]

[[route]]
model = "default"                  # 兜底路由
steps = [{ provider = "anthropic" }]
```

匹配优先级：精确匹配 > 前缀通配符 > `default`。

### 回退策略

```toml
[fallback]
trigger_codes = [429, 500, 502, 503, 504, 529]  # 触发重试的状态码
max_retries = 3
initial_delay_ms = 500
max_delay_ms = 8000
circuit_breaker_threshold = 5       # 连续失败次数
circuit_breaker_cooldown_secs = 60  # 熔断冷却时间
```

### 日志

```toml
[logging]
level = "info"    # trace, debug, info, warn, error
format = "json"   # json 或 text
```

## API

| 方法   | 路径            | 说明                         |
|--------|-----------------|------------------------------|
| POST   | `/v1/messages`  | Anthropic Messages API 透传  |
| GET    | `/health`       | 健康检查，返回 `{"status":"ok"}` |

请求需包含 `x-api-key` 头（如果配置了 `api_key`）。`anthropic-version` 和 `anthropic-beta` 头原样透传。

响应包含 `x-zrouter-provider` 头，指示实际处理请求的 provider。

## TLS

**自定义证书：**

```toml
[server]
tls = true
cert_file = "/etc/zrouter/cert.pem"
key_file = "/etc/zrouter/key.pem"
```

**开发模式自签名证书：**

```toml
[server]
tls = true
# 不设置 cert_file / key_file
```

首次启动自动生成并保存到配置文件同目录下：
- `zrouter-dev-cert.pem` — 证书（SAN: localhost, 127.0.0.1, ::1）
- `zrouter-dev-key.pem` — 私钥（0600 权限）

后续启动复用同一证书。将 `zrouter-dev-cert.pem` 导入系统信任库即可让 AI 工具正常连接：

```bash
# Fedora/RHEL
sudo trust anchor zrouter-dev-cert.pem

# Ubuntu/Debian
sudo cp zrouter-dev-cert.pem /usr/local/share/ca-certificates/zrouter-dev.crt
sudo update-ca-certificates
```

## systemd

```bash
sudo cp contrib/zrouter.service /etc/systemd/system/
sudo cp config.example.toml /etc/zrouter/config.toml

# 配置 API Key
sudo tee /etc/zrouter/env <<EOF
ANTHROPIC_API_KEY=sk-ant-...
OPENROUTER_API_KEY=sk-or-...
EOF
sudo chmod 600 /etc/zrouter/env

sudo systemctl daemon-reload
sudo systemctl enable --now zrouter
```

## 架构

```
请求 → TLS 握手 → API Key 验证 → 提取 model → 路由匹配
                                            ↓
                                    回退执行器 → provider1 → provider2 → ...
                                            ↓
                                     熔断器检查 → 请求上游 → 重试/切换
```

| 模块             | 职责                                    |
|------------------|-----------------------------------------|
| `main.rs`        | 守护进程生命周期，信号处理              |
| `config.rs`      | TOML 解析与校验                         |
| `server.rs`      | hyper HTTP 服务器，请求分发             |
| `tls.rs`         | TLS 配置，PEM 加载，自签名证书生成      |
| `proxy.rs`       | JSON body 中 model 字段提取/替换        |
| `router.rs`      | 路由匹配（精确 > 前缀 > 默认）         |
| `provider.rs`    | Provider 注册表，原子熔断器             |
| `fallback.rs`    | 重试循环，指数退避                      |
| `auth.rs`        | API Key constant-time 验证             |
| `logging.rs`     | 结构化日志，本地时区时间戳              |

## 错误响应格式

```json
{
  "type": "error",
  "error": {
    "type": "authentication_error",
    "message": "Invalid API key"
  }
}
```

| HTTP 状态 | error type            | 场景                       |
|-----------|----------------------|----------------------------|
| 400       | `invalid_request_error` | 缺少 model 或 body 读取失败 |
| 401       | `authentication_error`  | API Key 无效                |
| 404       | `not_found_error`       | 路径错误或无匹配路由        |
| 500       | `api_error`             | 上游返回非触发码致命错误    |
| 503       | `overloaded_error`      | 所有 provider 耗尽          |

## 依赖

17 个 crate，最小化依赖原则。

## 许可证

参见项目根目录 LICENSE 文件。
