# AI Gateway Service

External Redis-backed service used by the `ai-gateway-queue` Proxy-Wasm plugin.

It keeps Redis, worker execution, Pub/Sub waiting, callback delivery, and result storage outside the wasm sandbox.

## Endpoints

- `POST /v1/ratelimit/check`
  - Reads `X-Tenant-Id`, `X-Model`, and `X-Original-Path`.
  - Runs a Redis Lua token bucket.
  - Can override per tenant with Redis keys `ai:tenant:ratelimit:{tenant}:rps` and `ai:tenant:ratelimit:{tenant}:burst`.
  - Returns `{ "allowed": bool, "retry_after_ms": number }`.
- `POST /v1/queue/enqueue`
  - Requires `X-Callback-URL` by default.
  - Streams the request body, then stores either inline base64 body or an object-store reference in Redis Stream.
  - Returns `202 Accepted` with `X-Job-Id`.
- `POST /v1/queue/enqueue-and-wait`
  - Enqueues the job and waits for the worker result via Redis Pub/Sub.
  - Returns the upstream response with `X-Job-Id` and `X-Queue-Wait-Ms`, or `504`.
- `GET /v1/jobs/{job_id}`
  - Returns the stored result JSON while the result key TTL is alive.
- `GET /metrics`
  - Returns Prometheus text metrics for queue depth, limits, callbacks, retries, and worker counters.

## Run

```bash
cargo run -p ai-gateway-service -- \
  --redis-url redis://127.0.0.1/ \
  --upstream-base-url http://127.0.0.1:9000
```

Useful environment variables:

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
AI_REQUIRE_HTTPS_CALLBACK=true
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

Priority queues are disabled by default. Enable them and send `X-Queue-Priority: high|low` to route jobs to separate streams:

```bash
AI_ENABLE_PRIORITY_STREAMS=true
AI_QUEUE_HIGH_STREAM=ai:jobs:high
AI_QUEUE_LOW_STREAM=ai:jobs:low
```

Callback failures are written to `AI_CALLBACK_RETRY_STREAM` and retried by a local retry worker. Pending Redis Stream jobs are reclaimed with `XAUTOCLAIM` according to the reclaim settings.
