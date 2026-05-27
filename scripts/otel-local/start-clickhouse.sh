#!/usr/bin/env bash
set -euo pipefail

NAME="${SPACEGATE_CLICKHOUSE_NAME:-spacegate-clickhouse}"
NETWORK="${SPACEGATE_OTEL_NETWORK:-spacegate-otel}"
DATA_DIR="${SPACEGATE_CLICKHOUSE_DATA_DIR:-/tmp/spacegate-otel/clickhouse}"

mkdir -p "$DATA_DIR"

if ! docker network inspect "$NETWORK" >/dev/null 2>&1; then
  docker network create "$NETWORK" >/dev/null
fi

if docker ps -a --format '{{.Names}}' | grep -qx "$NAME"; then
  docker rm -f "$NAME" >/dev/null
fi

docker run --rm --name "$NAME" \
  --network "$NETWORK" \
  -p 28123:8123 \
  -p 29000:9000 \
  -e CLICKHOUSE_DB=otel \
  -e CLICKHOUSE_USER=default \
  -e CLICKHOUSE_PASSWORD= \
  -e CLICKHOUSE_DEFAULT_ACCESS_MANAGEMENT=1 \
  -v "$DATA_DIR:/var/lib/clickhouse" \
  clickhouse/clickhouse-server:latest
