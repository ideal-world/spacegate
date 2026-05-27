#!/usr/bin/env bash
set -euo pipefail

BASE_DIR="${SPACEGATE_OTEL_DIR:-/tmp/spacegate-otel}"
NAME="${SPACEGATE_OTEL_COLLECTOR_NAME:-spacegate-otel}"
NETWORK="${SPACEGATE_OTEL_NETWORK:-spacegate-otel}"
CONFIG="$BASE_DIR/otel-collector.yaml"

if [ ! -f "$CONFIG" ]; then
  "$(dirname "$0")/prepare-config.sh"
fi

if docker ps -a --format '{{.Names}}' | grep -qx "$NAME"; then
  docker rm -f "$NAME" >/dev/null
fi

if ! docker network inspect "$NETWORK" >/dev/null 2>&1; then
  docker network create "$NETWORK" >/dev/null
fi

docker run --rm --name "$NAME" \
  --network "$NETWORK" \
  -p 4317:4317 \
  -p 4318:4318 \
  -v "$CONFIG:/etc/otelcol-contrib/config.yaml:ro" \
  otel/opentelemetry-collector-contrib:0.152.1
