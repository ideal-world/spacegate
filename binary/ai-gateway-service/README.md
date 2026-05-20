# AI Gateway Service

External Redis-backed service used by the `ai-gateway-queue` Proxy-Wasm plugin.

It keeps Redis, worker execution, Pub/Sub waiting, callback delivery, and result storage outside the wasm sandbox.

## Endpoints

- `POST /v1/ratelimit/check`
  - Reads `X-Tenant-Id`, `X-Model`, and `X-Original-Path`.
  - Runs a Redis Lua token bucket.
  - Returns `{ "allowed": bool, "retry_after_ms": number }`.
- `POST /v1/queue/enqueue`
  - Stores the raw request body and selected headers in Redis Stream.
  - Returns `202 Accepted` with `X-Job-Id`.
- `POST /v1/queue/enqueue-and-wait`
  - Enqueues the job and waits for the worker result via Redis Pub/Sub.
  - Returns the upstream response or `504`.
- `GET /v1/jobs/{job_id}`
  - Returns the stored result JSON while the result key TTL is alive.

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
```

