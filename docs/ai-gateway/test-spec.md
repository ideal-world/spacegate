# AI Gateway 队列 — 测试用例规格

**设计依据：** 原始设计文档未纳入本仓库；本文件以当前实现约定为准。  
**语义基准：** DOC-01/02 定稿（配额内三种策略均直通上游；超额时 abandon→429 / queue→202 / wait→阻塞或 504）

---

## Traceability 矩阵

| 设计文档章节 | 用例 ID | 自动化 |
|-------------|---------|--------|
| §限流策略 / 请求头 | TC-HDR-* | Rust IT / Hurl / GW E2E |
| §abandon 示例 | TC-AB-* | Rust IT / Hurl / GW E2E |
| §queue 示例 / 时序 | TC-Q-* | Rust IT / Hurl |
| §wait 示例 / 时序 | TC-W-* | Rust IT / Hurl |
| §核心组件 §1 限流器 | TC-RL-* | Rust IT / Hurl |
| §核心组件 §2 Body | TC-BODY-* | Rust IT / MinIO E2E |
| §核心组件 §3 Stream | TC-Q-* / TC-WK-* | Rust IT |
| §核心组件 §4 Pub/Sub | TC-W-* | Rust IT |
| §性能设计 | TC-BODY-05/07, TC-W-06 | Rust IT |
| §可靠性 | TC-WK-* | Rust IT |
| §监控指标 | TC-MET-* | Rust IT / Hurl |
| §部署 | TC-DEP-* | Shell |
| Wasm 网关层 | TC-GW-* | GW E2E（可选） |

**图例：** Rust IT = `cargo test --test integration`；Hurl = `tests/hurl/*.hurl`；GW E2E = `scripts/run-gateway-e2e.sh`

---

## 1. 请求头与策略（TC-HDR）

### TC-HDR-01 缺 Policy 且无 default

- **设计映射：** §限流策略 — `X-RateLimit-Policy` 必填
- **前置：** Wasm `default_policy=null`；Service 直接调用入队接口
- **步骤：** POST `/v1/queue/enqueue`，不带 `x-ratelimit-policy`
- **期望：** 400；Service 侧 bad request（若直接打 service 则 policy 可选但 Wasm 层 400）

### TC-HDR-02 Policy 非法值

- **步骤：** `x-ratelimit-policy: invalid`
- **期望：** Wasm 400 `missing_or_invalid_rate_limit_policy`

### TC-HDR-03 缺 X-Tenant-Id

- **步骤：** 任意策略，不带 tenant
- **期望：** Wasm 400 `missing_x_tenant_id`；Service `/v1/ratelimit/check` 400

### TC-HDR-04 queue 缺 X-Callback-URL

- **步骤：** POST `/v1/queue/enqueue`，policy=queue，无 callback
- **期望：** 400 `missing required header x-callback-url`

### TC-HDR-05 queue 回调非 HTTPS（生产配置）

- **前置：** `require_https_callback=true`
- **步骤：** `x-callback-url: http://example.com/cb`
- **期望：** 400 `x-callback-url must use https`

### TC-HDR-06 wait 默认 timeout 60s

- **前置：** 上游/mock 延迟 >60s；`wait_timeout_secs=60`
- **步骤：** wait 入队并等待
- **期望：** 504；JSON 含 `error=timeout`、`waited_ms`≈60000

### TC-HDR-07 wait 自定义 X-Request-Timeout

- **步骤：** `x-request-timeout: 2`（测试配置缩短）
- **期望：** ~2s 后 504

---

## 2. 限流器（TC-RL）

### TC-RL-01 租户隔离

- **设计映射：** §核心组件 §1 — 限流粒度按 X-Tenant-Id
- **前置：** RPS=1, burst=1
- **步骤：** tenant-A 连续 2 次 check；tenant-B 1 次 check
- **期望：** A 第二次 `allowed=false`；B 第一次 `allowed=true`

### TC-RL-02 配额内 allowed

- **步骤：** 首次 check
- **期望：** `{ "allowed": true, "retry_after_ms": 0 }`

### TC-RL-03 超额与指标

- **步骤：** 耗尽 burst 后再 check
- **期望：** `allowed=false`，`retry_after_ms>0`；`/metrics` 中 `rate_limited_total{policy,tenant}` +1

### TC-RL-04 burst 超发后拒绝

- **前置：** burst=2
- **步骤：** 连续 3 次 check（同 tenant）
- **期望：** 前 2 次 allowed，第 3 次 denied

### TC-RL-05 Admin 租户规则覆盖

- **步骤：** PUT `/v1/admin/tenant-rate-limits` 设置 tenant 低 RPS；再 check
- **期望：** 新 RPS 生效（更快触发 denied）

### TC-RL-06 规则 lookup 优先级

- **步骤：** 写入 tenant 全局规则 + tenant+model 更严格规则；带 model header check
- **期望：** 使用更具体规则

### TC-RL-07 Redis key tenant-only

- **步骤：** check 后 Redis KEYS `ai:ratelimit:*`
- **期望：** 仅 `ai:ratelimit:{tenant}:tokens` 与 `:ts`；不含 model/path

---

## 3. abandon（TC-AB）

### TC-AB-01 配额内直通

- **设计映射：** §abandon — 未触发限流时正常返回 LLM 响应
- **步骤：** Wasm policy=abandon，配额内
- **期望：** 200，body 来自 upstream（非 202/429）

### TC-AB-02 超额 429

- **步骤：** 触发限流
- **期望：** 429；`Retry-After`；`{"error":"rate_limited","retry_after_ms":N}`

### TC-AB-03 不调用 enqueue

- **步骤：** 配额内 abandon；监控 service 日志/无 enqueue 指标增长
- **期望：** 无 `/v1/queue/enqueue` 调用

---

## 4. queue（TC-Q）

### TC-Q-01 配额内直通（定稿）

- **步骤：** Wasm policy=queue，配额内
- **期望：** 200 上游响应（**非**设计文档 queue 示例「永远 202」）

### TC-Q-02 超额 202 入队

- **步骤：** 超额 queue
- **期望：** 202；Header `X-Job-Id`；JSON `poll_url=/jobs/{id}/status`

### TC-Q-03 202 JSON 字段

- **期望：** `job_id` 为 ULID 格式；`status=queued`；`poll_url` 正确

### TC-Q-04 Worker 回调

- **前置：** mock callback server
- **步骤：** 超额入队 → worker 完成
- **期望：** POST 回调；Header `X-Gateway-Job-Id`

### TC-Q-05 回调 JSON 四字段

- **期望：** `{ job_id, status, result, completed_at }` 仅此四字段（result 为 LLM JSON）

### TC-Q-06 Stream entry 字段

- **步骤：** XREAD 或 XRANGE 读 stream
- **期望：** job_id, body/ref, size, policy, callback_url, headers, created_at 等齐全

### TC-Q-07 dev HTTP 回调

- **前置：** `require_https_callback=false`
- **步骤：** `http://` callback URL 入队
- **期望：** 202

---

## 5. wait（TC-W）

### TC-W-01 配额内直通

- **期望：** 200 上游响应，无入队等待

### TC-W-02 超额成功

- **步骤：** 超额 wait，worker 正常
- **期望：** 200；`X-Job-Id`；`X-Queue-Wait-Ms`；LLM body

### TC-W-03 竞态保险

- **前置：** worker 即时完成（0 延迟 upstream）
- **步骤：** enqueue-and-wait
- **期望：** 200（subscribe 前 result 已写入）

### TC-W-04 超时 504

- **前置：** upstream 延迟 > timeout
- **期望：** 504；`error/timeout/job_id/waited_ms/message`

### TC-W-05 504 后 poll

- **步骤：** 504 后等待 worker 完成；GET `/jobs/{id}/status`
- **期望：** 200 原始 LLM 响应体

### TC-W-06 Pub/Sub 连接复用 smoke

- **步骤：** 并发 N 个 wait（N=10 smoke）
- **期望：** 全部完成；Redis 连接数无 N 倍 subscriber 连接

---

## 6. Body 处理（TC-BODY）

### TC-BODY-01 inline ≤128KB

- **期望：** storage=inline；Redis entry 含 base64 body

### TC-BODY-02 S3 卸载 >128KB

- **前置：** 配置 object_store_endpoint
- **期望：** storage=object；entry 仅 ref；`object_offload_total`+1

### TC-BODY-03 无 S3 大 body

- **期望：** 413 Payload Too Large

### TC-BODY-04 超 MAX_BODY_BYTES

- **期望：** 413

### TC-BODY-05 S3 与 XADD 并发

- **期望：** 入队在合理时间内完成（相对串行基线）

### TC-BODY-06 multipart 失败 Abort

- **前置：** mock S3 返回 500
- **期望：** 入队失败；无成功 XADD

### TC-BODY-07 body Semaphore

- **前置：** body_read_concurrency=2（测试配置）
- **步骤：** 3 个并发大 body 入队
- **期望：** 第三个延迟开始（可选 smoke）

---

## 7. Worker / 可靠性（TC-WK）

### TC-WK-01 批量并发消费

- **步骤：** 一次 XADD 5 条；观察 worker 处理
- **期望：** 5 条均完成（并发处理）

### TC-WK-02 XAUTOCLAIM

- **前置：** reclaim_interval_secs=2（测试）；模拟 PEL 未 ACK
- **期望：** 重认领后重新处理

### TC-WK-03 回调失败 → retry stream

- **前置：** callback URL 不可达
- **期望：** callback_retry_stream 有 entry

### TC-WK-04 回调 DLQ

- **前置：** 超过 max retry
- **期望：** callback_dlq_stream 有 entry

### TC-WK-05 job DLQ

- **前置：** max_delivery_attempts=1；反复失败
- **期望：** job_dlq_stream

### TC-WK-06 result TTL 120s

- **前置：** result_ttl_secs=2（测试）
- **步骤：** 完成后等待 TTL；poll
- **期望：** 404 not_found

### TC-WK-07 优先级 Stream

- **前置：** enable_priority_streams=true；high/normal 均有积压
- **期望：** high 优先被消费完

---

## 8. 监控与部署（TC-MET / TC-DEP）

### TC-MET-01 metrics 基础

- **步骤：** GET `/metrics`
- **期望：** 200；含 `queue_depth`、`pel_size`

### TC-MET-02 rate_limited 标签

- **期望：** `rate_limited_total{policy="...",tenant="..."}` 行存在

### TC-MET-03 enqueue_latency 分桶

- **期望：** `enqueue_latency_ms_bucket{policy,size_bucket,le=...}` 存在

### TC-DEP-01 Redis 6 拒绝

- **步骤：** 对 Redis 6 启动 service
- **期望：** 启动失败，明确错误信息

### TC-DEP-02 Redis 7+ 通过

- **期望：** 正常启动

---

## 9. Wasm 网关层（TC-GW，可选）

### TC-GW-01 abandon 超额

- **期望：** 429

### TC-GW-02 queue 超额

- **期望：** 202

### TC-GW-03 wait 超额

- **期望：** 200 或 504（视 upstream 延迟）

### TC-GW-04 service 不可达

- **期望：** 502

---

## 运行命令

```bash
# 单元测试（无需 Redis）
cd spacegate && cargo test -p ai-gateway-service

# 集成测试（需 Redis 7+）
./spacegate/binary/ai-gateway-service/scripts/run-integration-tests.sh

# Hurl 黑盒
./spacegate/binary/ai-gateway-service/scripts/run-hurl-tests.sh

# MinIO E2E
./spacegate/binary/ai-gateway-service/scripts/queue-object-store-e2e.sh

# Wasm 策略逻辑（host）
cd spacegate/plugins/wasm/ai-gateway-queue && cargo test --lib
```

---

## 修订记录

| 日期 | 说明 |
|------|------|
| 2026-05-24 | 初版：55 条 TC-* 用例 + traceability |
| 2026-05-24 | 落地 Rust 集成测试 22 项、Hurl 5 文件、脚本 4 个、Wasm policy host 测试 3 项 |

## 已实现自动化映射

| 用例 ID | Rust IT | Hurl | 脚本 |
|---------|---------|------|------|
| TC-HDR-03~05 | body_store / enqueue_queue | queue | |
| TC-RL-01~07 | ratelimit / admin | ratelimit / admin | |
| TC-Q-02~07 | enqueue_queue | queue | |
| TC-W-02~05 | enqueue_wait | wait | |
| TC-BODY-01/03 | body_store | | |
| TC-WK-01/03 | worker_reliability | | |
| TC-MET-01/02, TC-DEP-02 | metrics | metrics | |
| TC-BODY-02 | | | queue-object-store-e2e.sh |
| TC-GW / TC-HDR-02 | policy host tests | | run-gateway-e2e.sh (stub) |
