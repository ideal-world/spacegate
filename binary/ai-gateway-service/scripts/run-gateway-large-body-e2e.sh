#!/usr/bin/env bash
# TC-GW-BODY-01 / TC-BODY-02：Docker 全栈大 body E2E
# 流量路径：Client -> SpaceGate(:9993) Wasm -> ai-gateway-service -> MinIO + Redis
#
# 前置：
#   docker compose -f docker-compose.yml -f docker-compose.queue.yml --profile queue up -d
#   ./scripts/sync-wasm-plugin-to-docker-config.sh && 重启 spacegate
#
# 用法：
#   ./spacegate/binary/ai-gateway-service/scripts/run-gateway-large-body-e2e.sh
#   ENSURE_STACK=1 ./spacegate/.../run-gateway-large-body-e2e.sh   # 自动 compose up
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/../../../.." && pwd)"
cd "$ROOT"

GATEWAY="${GATEWAY_URL:-http://127.0.0.1:9993}"
SERVICE="${SERVICE_URL:-http://127.0.0.1:18080}"
MINIO_HOST="${MINIO_HOST:-http://127.0.0.1:9010}"
MINIO_USER="${MINIO_ROOT_USER:-minioadmin}"
MINIO_PASS="${MINIO_ROOT_PASSWORD:-minioadmin}"
BUCKET="${AI_OBJECT_STORE_BUCKET:-ai-gateway-body}"
# 200 KiB，超过默认 inline 阈值 128 KiB
BODY_SIZE="${LARGE_BODY_BYTES:-204800}"
TENANT="gw-large-body-${RANDOM}"
COMPOSE=(docker compose -f docker-compose.yml -f docker-compose.queue.yml --profile queue)

die() { echo "ERROR: $*" >&2; exit 1; }

metric_value() {
  local name="$1"
  curl -sf "$SERVICE/metrics" | awk -v n="$name" '$1 == n { print $2; exit }'
}

wait_http() {
  local url="$1" retries="${2:-30}"
  for _ in $(seq 1 "$retries"); do
    curl -sf "$url" >/dev/null 2>&1 && return 0
    sleep 1
  done
  return 1
}

if [[ "${ENSURE_STACK:-0}" == "1" ]]; then
  echo "==> 启动 / 更新 Docker 栈（queue profile）"
  export DOCKER_BUILDKIT=1
  if [[ -x ./scripts/sync-wasm-plugin-to-docker-config.sh ]]; then
    ./scripts/sync-wasm-plugin-to-docker-config.sh
  fi
  "${COMPOSE[@]}" up -d --build minio minio-init ai-gateway-service spacegate admin-server
fi

echo "==> 前置检查"
wait_http "$SERVICE/healthz" || die "ai-gateway-service 不可达: $SERVICE"
wait_http "http://127.0.0.1:19880/health" || die "SpaceGate 不可达"
curl -sf "$MINIO_HOST/minio/health/live" >/dev/null || die "MinIO 不可达: $MINIO_HOST"

echo "==> 配置租户限流（burst=1，便于触发 queue 超额入队）"
curl -sf -X PUT "$SERVICE/v1/admin/tenant-rate-limits" \
  -H 'Content-Type: application/json' \
  -d "{\"tenant\":\"$TENANT\",\"rps\":1,\"burst\":1}" >/dev/null

BASELINE="$(metric_value object_offload_total || echo 0)"

echo "==> 消耗令牌（abandon 配额内 1 次）"
code=$(curl -s -o /dev/null -w '%{http_code}' -X POST "$GATEWAY/v1/chat/completions" \
  -H 'X-RateLimit-Policy: abandon' \
  -H "X-Tenant-Id: $TENANT" \
  -H 'Content-Type: application/json' \
  -d '{"warmup":true}')
[[ "$code" == "200" ]] || die "预热请求期望 200，实际 $code"

echo "==> 经网关发送大 body（queue 策略，应 202 入队）"
LARGE="$(python3 -c "print('x'*${BODY_SIZE})")"
# 回调走 Docker 内 mock-upstream，service 容器可直接访问
CALLBACK="${CALLBACK_URL:-http://mock-upstream:9000/callback}"

http=$(curl -sS -o /tmp/gw-large-body.json -w '%{http_code}' \
  -X POST "$GATEWAY/v1/chat/completions" \
  -H "X-Tenant-Id: $TENANT" \
  -H 'X-RateLimit-Policy: queue' \
  -H "X-Callback-URL: $CALLBACK" \
  -H 'Content-Type: application/octet-stream' \
  --data-binary "$LARGE")
[[ "$http" == "202" ]] || die "大 body 入队期望 202，实际 $http body=$(cat /tmp/gw-large-body.json)"

echo "==> 等待 object_offload_total 递增"
found=0
for _ in $(seq 1 45); do
  now="$(metric_value object_offload_total || echo 0)"
  if awk -v a="$BASELINE" -v b="$now" 'BEGIN{exit !(b>a)}'; then
    echo "object_offload_total: $BASELINE -> $now"
    found=1
    break
  fi
  sleep 1
done
[[ "$found" == "1" ]] || die "object_offload_total 未递增（baseline=$BASELINE）"

echo "==> 验证 MinIO bucket 内有对象"
obj_count=$(docker run --rm --network ai-gateway-net --entrypoint /bin/sh minio/mc:latest \
  -c "
    mc alias set local http://minio:9000 '$MINIO_USER' '$MINIO_PASS' >/dev/null &&
    mc ls -r local/$BUCKET/bodies 2>/dev/null | wc -l | tr -d ' '
  ")
[[ "${obj_count:-0}" -gt 0 ]] || die "MinIO bucket/$BUCKET 下未发现 bodies/ 对象"

echo "==> 全栈大 body E2E 通过（tenant=$TENANT, body=${BODY_SIZE}B, minio_objects>=1）"
