#!/usr/bin/env bash
# 运行 Rust 集成测试（需 Redis 7+）
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT/../.."

REDIS_URL="${REDIS_URL:-redis://127.0.0.1/}"
export REDIS_URL

echo "Checking Redis at $REDIS_URL ..."
redis_ok=false
if command -v redis-cli >/dev/null 2>&1; then
  if redis-cli -u "$REDIS_URL" INFO server 2>/dev/null | grep -qE 'redis_version:(7|[89])'; then
    redis_ok=true
  fi
elif docker exec ai-gateway-redis redis-cli INFO server 2>/dev/null | grep -qE 'redis_version:(7|[89])'; then
  redis_ok=true
elif nc -z 127.0.0.1 6379 2>/dev/null; then
  redis_ok=true
fi
if [[ "$redis_ok" != true ]]; then
  echo "ERROR: Redis 7+ required. Start redis or set REDIS_URL." >&2
  exit 1
fi

cargo test -p ai-gateway-service --features test-support --test integration "$@"
