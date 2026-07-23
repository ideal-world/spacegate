# AI Gateway Service

External Redis-backed service used by the `ai-gateway-queue` Proxy-Wasm plugin.

It keeps Redis, worker execution, Pub/Sub waiting, callback delivery, and result storage outside the wasm sandbox.

## Endpoints

- `POST /v1/ratelimit/check`
  - Reads `X-Tenant-Id`, optional `X-Model`, `X-Original-Path`, and `X-RateLimit-Policy`.
  - Runs a Redis Lua token bucket keyed by **tenant only** (`ai:ratelimit:{tenant}:tokens/ts`).
  - Per-tenant overrides via Admin API or Redis keys under `ai:tenant:ratelimit:{tenant}[:model:...][:path:...][:policy:...]`.
  - Returns `{ "allowed": bool, "retry_after_ms": number }`. Wasm calls this for **all** policies before enqueue or upstream passthrough.
- `POST /v1/queue/enqueue`
  - Requires `X-Callback-URL` by default.
  - Streams the request body, then stores either inline base64 body or an object-store reference in Redis Stream.
  - Returns `202 Accepted` with `X-Job-Id`.
- `POST /v1/queue/enqueue-and-wait`
  - Enqueues the job and waits for the worker result via Redis Pub/Sub.
  - Returns the upstream response with `X-Job-Id` and `X-Queue-Wait-Ms`, or `504`.
- `GET /v1/jobs/{job_id}` / `GET /jobs/{job_id}/status`
  - When the job is completed, returns the **raw upstream HTTP response** (status, headers, body) with `X-Job-Id`.
  - While pending or on error, returns JSON status metadata.
- `GET /metrics`
  - Returns Prometheus text metrics for queue depth, PEL size, DLQ depth, enqueue latency, body size, waits, limits, callbacks, retries, object offload, and worker counters.

## Run

```bash
cargo run -p ai-gateway-service -- \
  --redis-url redis://127.0.0.1/ \
  --upstream-base-url http://127.0.0.1:9000
```

Or use a TOML config file (recommended for local / deployment):

```bash
cargo run -p ai-gateway-service -- --config config/ai-gateway-service.toml
```

If `--config` / `AI_GATEWAY_CONFIG` is omitted, the service looks for `ai-gateway-service.toml` in the **same directory as the executable**. For deployment, place the binary and config file together:

```text
/opt/ai-gateway/
  ai-gateway-service          # binary
  ai-gateway-service.toml     # auto-loaded
```

Example configs live under `config/`:

- `config/ai-gateway-service.example.toml` — full reference with all sections
- `config/ai-gateway-service.toml` — minimal local dev template

Precedence: explicit CLI flags / environment variables > config file > built-in defaults.

Default config discovery order:

1. `--config` or `AI_GATEWAY_CONFIG`
2. `{executable_dir}/ai-gateway-service.toml` (if the file exists)
3. Built-in defaults only

Set the config path via environment variable:

```bash
AI_GATEWAY_CONFIG=config/ai-gateway-service.toml cargo run -p ai-gateway-service
```

Useful environment variables:

```bash
REDIS_URL=redis://127.0.0.1/
AI_UPSTREAM_BASE_URL=http://127.0.0.1:9000
AI_RATE_LIMIT_RPS=100
AI_RATE_LIMIT_BURST=200
AI_RATE_LIMIT_COST=1
AI_WAIT_TIMEOUT_SECS=60
AI_WORKER_CONCURRENCY=4
AI_MAX_BODY_BYTES=33554432
AI_INLINE_THRESHOLD=131072
AI_QUEUE_MAX_LEN=100000
AI_ENABLE_PRIORITY_STREAMS=true
AI_QUEUE_DEFAULT_PRIORITY=normal
AI_QUEUE_HIGH_MODELS=gpt-4o,qwen-max
AI_QUEUE_LOW_TENANTS=free
AI_QUEUE_HIGH_WEIGHT=3
AI_QUEUE_NORMAL_WEIGHT=1
AI_QUEUE_LOW_WEIGHT=1
AI_RECLAIM_INTERVAL_SECS=30
AI_RECLAIM_MIN_IDLE_SECS=30
AI_JOB_PROCESS_LEASE_SECS=120
AI_JOB_MAX_DELIVERY_ATTEMPTS=5
AI_REQUIRE_HTTPS_CALLBACK=true
AI_CALLBACK_MAX_RETRY_ATTEMPTS=5
AI_CALLBACK_RETRY_INITIAL_DELAY_MS=1000
AI_CALLBACK_RETRY_MAX_DELAY_MS=60000
AI_CALLBACK_RETRY_RECLAIM_IDLE_SECS=60
```

Optional object offload variables:

```bash
AI_OBJECT_STORE_ENDPOINT=http://127.0.0.1:9000
AI_OBJECT_STORE_BUCKET=ai-gateway-body
AI_OBJECT_STORE_PREFIX=bodies
AI_OBJECT_MULTIPART_PART_SIZE=5242880
AI_OBJECT_STORE_AUTH_HEADER='Authorization: Bearer token'
```

Request body reading is streaming. The service accumulates only the inline buffer until `AI_INLINE_THRESHOLD`; after that it starts multipart upload and flushes parts as `AI_OBJECT_MULTIPART_PART_SIZE` chunks become available. `AI_MAX_BODY_BYTES` is enforced while reading the stream.

When `AI_OBJECT_STORE_ENDPOINT` is set and the body is larger than `AI_INLINE_THRESHOLD`, the service uses the S3-compatible multipart flow:

```text
CreateMultipartUpload -> UploadPart* -> CompleteMultipartUpload
```

If any part upload or completion fails, the service sends `AbortMultipartUpload` before returning the enqueue error. The current implementation expects a MinIO/S3-compatible endpoint that accepts either unsigned requests or the configured static auth header.

Tenant rate-limit overrides (Admin API + Redis):

```text
GET/PUT/DELETE /v1/admin/tenant-rate-limits
```

Redis key patterns (most specific match wins; token bucket remains tenant-scoped):

```text
ai:tenant:ratelimit:{tenant}:model:{model}:path:{path}:policy:{policy}
ai:tenant:ratelimit:{tenant}:model:{model}:path:{path}
ai:tenant:ratelimit:{tenant}:model:{model}:policy:{policy}
ai:tenant:ratelimit:{tenant}:path:{path}:policy:{policy}
ai:tenant:ratelimit:{tenant}:model:{model}
ai:tenant:ratelimit:{tenant}:path:{path}
ai:tenant:ratelimit:{tenant}:policy:{policy}
ai:tenant:ratelimit:{tenant}
```

JSON value:

```json
{"rps": 20, "burst": 40, "cost": 1}
```

CSV value:

```text
20,40,1
```

The old per-tenant keys are still supported as fallback: `ai:tenant:ratelimit:{tenant}:rps`, `:burst`, and `:cost`.

Global defaults when no tenant rule matches:

```bash
AI_RATE_LIMIT_RPS=100
AI_RATE_LIMIT_BURST=200
AI_RATE_LIMIT_COST=1
```

The Wasm plugin invokes `/v1/ratelimit/check` for **abandon**, **queue**, and **wait** before passthrough or enqueue.

Priority streams are **enabled by default** (`AI_ENABLE_PRIORITY_STREAMS=true`). Send `X-Queue-Priority: high|normal|low` to route jobs to separate streams, or configure model/tenant defaults:

```bash
AI_ENABLE_PRIORITY_STREAMS=true
AI_QUEUE_HIGH_STREAM=ai:jobs:high
AI_QUEUE_LOW_STREAM=ai:jobs:low
AI_QUEUE_HIGH_MODELS=gpt-4o,qwen-max
AI_QUEUE_LOW_TENANTS=free
```

Workers consume streams in weighted order. `AI_QUEUE_HIGH_WEIGHT`, `AI_QUEUE_NORMAL_WEIGHT`, and `AI_QUEUE_LOW_WEIGHT` control how often each priority stream is checked per loop.

Callback failures are written to `AI_CALLBACK_RETRY_STREAM` with `attempt`, `next_attempt_at_ms`, and `last_error`. The retry worker uses exponential backoff capped by `AI_CALLBACK_RETRY_MAX_DELAY_MS`, ACKs each retry record after handling it, and moves exhausted callbacks to `AI_CALLBACK_DLQ_STREAM`. Pending Redis Stream jobs are reclaimed with `XAUTOCLAIM` according to the reclaim settings.

For job processing, each entry acquires a Redis lease key before upstream execution. Reclaimed entries that are already leased are skipped instead of being reprocessed, and jobs exceeding `AI_JOB_MAX_DELIVERY_ATTEMPTS` are moved to `AI_JOB_DLQ_STREAM`.

`/metrics` includes the core signals needed to operate the queue:

- `queue_depth`, `queue_depth{priority="high|low"}` for stream backlog.
- `pel_size`, `pel_size{priority="high|low"}`, and `callback_retry_pel_size` for unacked pending entries.
- `job_dlq_depth` and `callback_dlq_depth` for exhausted jobs and callbacks.
- `enqueue_latency_ms_*`, `enqueue_body_size_bytes_*`, `wait_total`, and `wait_timeout_total` for ingress and wait-mode health.
- `worker_processing_time_ms_*`, `worker_completed_total`, `worker_failed_total`, `reclaimed_total`, `lease_skip_total`, and `job_dlq_total` for worker health.
- `object_offload_total` and `object_multipart_abort_total` for large-body offload.

## Body offload tests

Unit tests (mock S3 multipart server, no Docker):

```bash
cargo test -p ai-gateway-service store_body_
```

## 测试规格与集成测试

完整用例规格见 [`spacegate/docs/ai-gateway/test-spec.md`](../../docs/ai-gateway/test-spec.md)（TC-* 编号，映射设计文档章节）。

```bash
# 单元测试（无需 Redis）
cd spacegate && cargo test -p ai-gateway-service

# Rust 集成测试（需 Redis 7+）
./spacegate/binary/ai-gateway-service/scripts/run-integration-tests.sh

# Hurl 黑盒（需 hurl + Redis + 编译 release binary）
./spacegate/binary/ai-gateway-service/scripts/run-hurl-tests.sh

# Wasm 策略纯逻辑（host 侧）
./spacegate/binary/ai-gateway-service/scripts/run-wasm-policy-tests.sh
```

MinIO end-to-end (Docker + worker roundtrip):

```bash
# 需要：Redis、mock 上游 :9000、Docker
./tests/queue-object-store-e2e.sh
```

The script starts MinIO on `:9001` by default (avoids clashing with the mock upstream on `:9000`), launches a dedicated `ai-gateway-service` on `:18081` with `AI_OBJECT_STORE_ENDPOINT`, and verifies:

- inline body below `AI_INLINE_THRESHOLD` does not increment `object_offload_total`
- larger body is stored in MinIO and the worker completes after `load_body()` fetches it
