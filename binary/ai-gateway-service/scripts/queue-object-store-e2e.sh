#!/usr/bin/env bash
# MinIO + 大 body E2E（TC-BODY-02 smoke）
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
WORKSPACE="$(cd "$ROOT/../.." && pwd)"
cd "$WORKSPACE"

REDIS_URL="${REDIS_URL:-redis://127.0.0.1/}"
MINIO_PORT="${MINIO_PORT:-9001}"
SVC_PORT="${E2E_SERVICE_PORT:-18081}"

if ! command -v docker >/dev/null 2>&1; then
  echo "SKIP: docker not available" >&2
  exit 0
fi

docker rm -f ai-gateway-minio-e2e 2>/dev/null || true
docker run -d --name ai-gateway-minio-e2e \
  -p "${MINIO_PORT}:9000" \
  -e MINIO_ROOT_USER=minioadmin \
  -e MINIO_ROOT_PASSWORD=minioadmin \
  minio/minio server /data >/dev/null

cleanup() {
  docker rm -f ai-gateway-minio-e2e 2>/dev/null || true
  kill $SVC_PID $UP_PID 2>/dev/null || true
}
trap cleanup EXIT

sleep 2

# MinIO 需先创建 bucket 并设为 public（服务使用无 SigV4 的直传 HTTP）
if docker run --rm --network host --entrypoint /bin/sh minio/mc -c \
  "mc alias set local http://127.0.0.1:${MINIO_PORT} minioadmin minioadmin && \
   mc mb --ignore-existing local/ai-gateway-body && \
   mc anonymous set public local/ai-gateway-body" >/dev/null 2>&1; then
  echo "MinIO bucket ai-gateway-body ready (public)."
else
  echo "ERROR: MinIO bucket bootstrap failed" >&2
  exit 1
fi

python3 - <<PY &
import json
from http.server import BaseHTTPRequestHandler, HTTPServer
class H(BaseHTTPRequestHandler):
    def do_POST(self):
        n = int(self.headers.get("Content-Length", 0))
        self.rfile.read(n)
        body = json.dumps({"e2e": True}).encode()
        self.send_response(200)
        self.send_header("Content-Type", "application/json")
        self.end_headers()
        self.wfile.write(body)
    def log_message(self, *a): pass
HTTPServer(("127.0.0.1", 9000), H).serve_forever()
PY
UP_PID=$!

cargo build -q -p ai-gateway-service --release
SVC="$WORKSPACE/target/release/ai-gateway-service"

AI_OBJECT_STORE_ENDPOINT="http://127.0.0.1:${MINIO_PORT}" \
AI_INLINE_THRESHOLD=1024 \
AI_REQUIRE_HTTPS_CALLBACK=false \
"$SVC" \
  --redis-url "$REDIS_URL" \
  --port "$SVC_PORT" \
  --host 127.0.0.1 \
  --upstream-base-url http://127.0.0.1:9000 \
  --object-store-endpoint "http://127.0.0.1:${MINIO_PORT}" \
  &
SVC_PID=$!
sleep 1

LARGE=$(python3 -c "print('x'*5000)")
JOB=$(curl -sS -o /tmp/e2e-enq.json -w '%{http_code}' \
  -X POST "http://127.0.0.1:${SVC_PORT}/v1/queue/enqueue" \
  -H 'X-Tenant-Id: e2e' \
  -H 'X-RateLimit-Policy: queue' \
  -H 'X-Callback-URL: http://127.0.0.1:9002/cb' \
  -H 'Content-Type: application/octet-stream' \
  --data-binary "$LARGE")
test "$JOB" = "202"

for i in $(seq 1 30); do
  MID=$(curl -sS "http://127.0.0.1:${SVC_PORT}/metrics")
  echo "$MID" | grep -q 'object_offload_total 1' && break
  sleep 1
done
echo "$MID" | grep -q 'object_offload_total 1'
echo "queue-object-store-e2e passed."
