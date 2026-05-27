#!/usr/bin/env bash
# SpaceGate + Wasm 全链路 E2E（TC-GW-* / TC-GW-BODY-*）
set -euo pipefail
DIR="$(cd "$(dirname "$0")" && pwd)"
exec "$DIR/run-gateway-large-body-e2e.sh" "$@"
