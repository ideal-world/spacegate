#!/usr/bin/env bash
# Hurl 黑盒测试：启动 mock 上游/回调 + ai-gateway-service
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
WORKSPACE="$(cd "$ROOT/../.." && pwd)"
cd "$WORKSPACE"

REDIS_URL="${REDIS_URL:-redis://127.0.0.1/}"
export REDIS_URL

if ! command -v hurl >/dev/null 2>&1; then
  echo "ERROR: hurl not installed. See https://hurl.dev" >&2
  exit 1
fi

redis_ok=false
if command -v redis-cli >/dev/null 2>&1 && redis-cli -u "$REDIS_URL" PING >/dev/null 2>&1; then
  redis_ok=true
elif docker exec ai-gateway-redis redis-cli PING >/dev/null 2>&1; then
  redis_ok=true
elif nc -z 127.0.0.1 6379 2>/dev/null; then
  redis_ok=true
fi
if [[ "$redis_ok" != true ]]; then
  echo "ERROR: Redis not reachable at $REDIS_URL" >&2
  exit 1
fi

# mock upstream :9000
python3 - <<'PY' &
import json
from http.server import BaseHTTPRequestHandler, HTTPServer

class H(BaseHTTPRequestHandler):
    def do_POST(self):
        body = json.dumps({"upstream": True, "hurl": True}).encode()
        self.send_response(200)
        self.send_header("Content-Type", "application/json")
        self.send_header("Content-Length", str(len(body)))
        self.end_headers()
        self.wfile.write(body)
    def log_message(self, *args): pass

HTTPServer(("127.0.0.1", 9000), H).serve_forever()
PY
UP_PID=$!

# mock callback
python3 - <<'PY' &
from http.server import BaseHTTPRequestHandler, HTTPServer

class H(BaseHTTPRequestHandler):
    def do_POST(self):
        n = int(self.headers.get("Content-Length", 0))
        self.rfile.read(n)
        self.send_response(200)
        self.end_headers()
    def log_message(self, *args): pass

HTTPServer(("127.0.0.1", 9002), H).serve_forever()
PY
CB_PID=$!

cleanup() {
  kill $UP_PID $CB_PID $SVC_PID 2>/dev/null || true
}
trap cleanup EXIT

cargo build -q -p ai-gateway-service --release
SVC="$WORKSPACE/target/release/ai-gateway-service"
PORT="${HURL_SERVICE_PORT:-18090}"
CALLBACK="http://127.0.0.1:9002/cb"

"$SVC" \
  --redis-url "$REDIS_URL" \
  --port "$PORT" \
  --host 127.0.0.1 \
  --upstream-base-url http://127.0.0.1:9000 \
  &
SVC_PID=$!
sleep 1

export service_url="http://127.0.0.1:${PORT}"
export callback_url="$CALLBACK"

hurl --test \
  --variable service_url="$service_url" \
  --variable callback_url="$callback_url" \
  --file-root "$ROOT/tests/fixtures" \
  "$ROOT/tests/hurl/ratelimit.hurl" \
  "$ROOT/tests/hurl/queue.hurl" \
  "$ROOT/tests/hurl/wait.hurl" \
  "$ROOT/tests/hurl/metrics.hurl" \
  "$ROOT/tests/hurl/admin.hurl"

echo "Hurl tests passed."
