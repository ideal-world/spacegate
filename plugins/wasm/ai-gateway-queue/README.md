# ai-gateway-queue

`ai-gateway-queue` 是一个运行在 SpaceGate Wasm 里的 **AI 请求队列网关**插件：在入口处对 AI 请求按租户做令牌桶限流，再根据 `X-RateLimit-Policy` 分流。**三种策略均先调用 `/v1/ratelimit/check`**；配额内直通上游，超额时分别返回 429 / 202 入队 / 入队并阻塞等待。

支持三种队列模式（通过 `X-RateLimit-Policy` 请求头选择，名字保留兼容历史）：

- `abandon`：配额内直通上游；超额返回 429（不入队）
- `queue`：配额内直通上游；超额入队后立即返回 `202`，结果通过回调或轮询拿到
- `wait`：配额内直通上游；超额入队后同步等待结果（类长轮询），超时返回 `504`

插件本身不直接访问 Redis，而是通过 `dispatch_http_call` 调用外部队列后端（`ai-gateway-service`），再由该后端处理 Redis Streams、worker 消费、回调重试、结果回收等队列基础设施。

## 架构

```text
Client
  -> SpaceGate / ai-gateway-queue wasm plugin
  -> ai-gateway-service
  -> Redis / Worker / Upstream AI Service
```

## 依赖

- SpaceGate 已启用 Wasm 支持
- Rust 工具链
- `wasm32-wasip1` 目标
- Redis
- `ai-gateway-service`

安装 wasm 目标：

```bash
rustup target add wasm32-wasip1
```

## 构建

在 `spacegate` 目录下执行：

```bash
cargo build --release --target wasm32-wasip1 --manifest-path plugins/wasm/Cargo.toml -p spacegate_plugin_ai_gateway_queue
```

编译产物：

```text
plugins/wasm/target/wasm32-wasip1/release/spacegate_plugin_ai_gateway_queue.wasm
```

## 启动外部服务

`ai-gateway-queue` 依赖外部服务来完成限流、入队、等待和回调。

```bash
cargo run -p ai-gateway-service -- \
  --redis-url redis://127.0.0.1/ \
  --upstream-base-url http://127.0.0.1:9000
```

常用环境变量：

```bash
REDIS_URL=redis://127.0.0.1/
AI_UPSTREAM_BASE_URL=http://127.0.0.1:9000
AI_RATE_LIMIT_RPS=100
AI_RATE_LIMIT_BURST=200
AI_WAIT_TIMEOUT_SECS=60
AI_WORKER_CONCURRENCY=4
AI_MAX_BODY_BYTES=33554432
AI_INLINE_THRESHOLD=131072
AI_QUEUE_MAX_LEN=100000
AI_RECLAIM_INTERVAL_SECS=30
AI_RECLAIM_MIN_IDLE_SECS=30
```

如果不设置 `AI_UPSTREAM_BASE_URL`，队列任务仍会写入 Redis，但不会由本地 worker 消费。

本地调试如果使用 HTTP 回调地址，可以临时加上：

```bash
AI_REQUIRE_HTTPS_CALLBACK=false
```

## SpaceGate 配置

可参考：

`/Users/sh.zhang/Workspace/[REDACTED]/jiyan/ai-gateway-dev/spacegate/resource/ai-gateway-demo/plugin/wasm.ai-gateway-queue.json`

关键配置项：

```json
{
  "url": "plugins/wasm/target/wasm32-wasip1/release/spacegate_plugin_ai_gateway_queue.wasm",
  "fail_strategy": "fail_close",
  "plugin_name": "ai-gateway-queue",
  "vm_pool_size": 4,
  "wait_vm_pool_size": 4,
  "limits": {
    "max_memory_pages": 64,
    "fuel_per_call": 20000000,
    "epoch_timeout_millis": 50,
    "max_body_bytes": 33554432,
    "max_pending_calls": 1
  },
  "plugin_config": {
    "service": {
      "cluster": "ai-gateway-service",
      "authority": "ai-gateway-service",
      "timeout_ms": 65000
    },
    "paths": {
      "rate_limit": "/v1/ratelimit/check",
      "enqueue": "/v1/queue/enqueue",
      "wait": "/v1/queue/enqueue-and-wait"
    },
    "headers": {
      "policy": "x-ratelimit-policy",
      "tenant": "x-tenant-id",
      "model": "x-model",
      "priority": "x-queue-priority"
    },
    "policies": {
      "require": true,
      "default": null
    },
    "priority": {
      "enabled": true,
      "default": "normal",
      "high_models": ["gpt-4o"],
      "low_tenants": ["free"]
    }
  },
  "clusters": {
    "ai-gateway-service": "http://127.0.0.1:18080"
  }
}
```

### `plugin_config` 说明

- `service_cluster`：外部服务所在 cluster 名称
- `service_authority`：转发时使用的 `:authority`
- `rate_limit_path`：限流检查接口
- `enqueue_path`：入队接口
- `wait_path`：入队并等待接口
- `service_timeout_ms`：调用外部服务超时
- `require_policy`：是否强制要求请求头携带策略
- `headers.*`：自定义客户端侧策略、租户、模型、优先级 header；插件会转成外部服务统一使用的 `x-ratelimit-policy`、`x-tenant-id`、`x-model`、`x-queue-priority`
- `policies.default`：未携带策略 header 时使用的默认策略；为空且 `require=true` 时会返回 `400`
- `priority.*`：插件侧优先级推导规则，支持按模型或租户自动设置 `high` / `low`

插件配置优先支持上面的结构化 JSON；旧的扁平字段仍兼容，例如 `service_cluster`、`rate_limit_path`、`tenant_header`、`default_policy`、`high_priority_models`。

## 请求头

插件依赖下列请求头：

- `X-RateLimit-Policy`：必填，取值为 `abandon`、`queue`、`wait`
- `X-Tenant-Id`：必填
- `X-Callback-URL`：`queue` 场景下必填，默认要求 HTTPS
- `X-Request-Timeout`：`wait` 场景下可选，单位为秒
- `X-Model`：可选，透传给外部服务
- `X-Queue-Priority`：可选，启用优先级队列后可传 `high` 或 `low`

Header 名称大小写不敏感；`X-RateLimit-Policy` 的值请使用小写。

## 三种模式

### 1. `abandon`

先调用限流接口，允许则继续转发到后端，拒绝则返回 `429`。

示例：

```bash
curl -i http://localhost:9080/your/api \
  -H 'X-RateLimit-Policy: abandon' \
  -H 'X-Tenant-Id: demo' \
  -H 'X-Model: gpt-4o-mini' \
  -d '{"prompt":"hello"}'
```

### 2. `queue`

先调用限流接口。配额内继续转发到上游（与 `abandon` 相同）；超额时请求体进入队列，插件返回 `202 Accepted`，响应里会带 `X-Job-Id`。

示例：

```bash
curl -i http://localhost:9080/your/api \
  -H 'X-RateLimit-Policy: queue' \
  -H 'X-Tenant-Id: demo' \
  -H 'X-Callback-URL: https://example.com/callback' \
  -d '{"prompt":"hello"}'
```

### 3. `wait`

先调用限流接口。配额内继续转发到上游；超额时请求体进入队列后等待结果返回。成功时直接返回上游响应，超时则返回 `504`。

示例：

```bash
curl -i http://localhost:9080/your/api \
  -H 'X-RateLimit-Policy: wait' \
  -H 'X-Tenant-Id: demo' \
  -H 'X-Request-Timeout: 60' \
  -d '{"prompt":"hello"}'
```

## 返回行为

- `400`：缺少必要请求头或策略非法（无 `X-RateLimit-Policy` 且无 `default_policy` 时也会 400）
- `429`：`abandon` 超额限流
- `202`：`queue` 超额入队已接收
- `200`/`4xx`/`5xx`：配额内三种策略均由上游返回；`wait` 超额完成后也由外部服务返回上游响应
- `502`：外部服务不可达或调用失败

`wait` 成功响应会带 `X-Job-Id` 和 `X-Queue-Wait-Ms`；`queue` 响应会带 `X-Job-Id` 和 `Location`。

## 生产化能力

- Redis Stream 支持 `MAXLEN ~` 裁剪，通过 `AI_QUEUE_MAX_LEN` 控制
- 租户限流支持按租户、模型、路由、策略多维覆盖，并支持单请求 cost
- 优先级队列支持 header、模型、租户规则推导，并由 worker 按权重消费高/普通/低优先级 Stream
- Worker 崩溃后通过 `XAUTOCLAIM` 重认领 pending job，并通过 Redis 处理租约避免长任务被重复执行
- 回调失败会进入 `AI_CALLBACK_RETRY_STREAM`，按指数退避重试，超过最大次数后进入 `AI_CALLBACK_DLQ_STREAM`
- 大 body 可通过 `AI_OBJECT_STORE_ENDPOINT` 走 S3-compatible multipart 卸载，Redis Stream 中只保留 `ref`；未配置 S3 且 body 超过 `AI_INLINE_THRESHOLD` 时返回 `413`
- 租户限流令牌桶按 `X-Tenant-Id` 隔离；可通过 Admin API `PUT /v1/admin/tenant-rate-limits` 或 Redis 配置键覆盖每租户 RPS/Burst（支持 model/path/policy 维度 lookup，桶 key 仍为 tenant-only）
- `/metrics` 暴露 Prometheus 文本指标，包含队列深度、PEL、DLQ、入队延迟、body 大小、wait 超时、回调重试和 worker 处理耗时

## 调试建议

- 先确认 `ai-gateway-service` 已启动并能连上 Redis
- 再确认 SpaceGate 的 `clusters.ai-gateway-service` 指向正确地址
- `wait` 模式建议单独使用 `wait_vm_pool_size`，避免拖垮普通请求
- 如果请求一直返回 `400 missing_or_invalid_rate_limit_policy`，先检查 `X-RateLimit-Policy`

## 备注

- 这个插件当前是面向 OpenAI 风格 AI 请求的队列和限流入口
- Redis 相关逻辑被放在 wasm 外部服务中，便于隔离和演进
- 具体协议和接口字段，以 `ai-gateway-service` 的实现为准
