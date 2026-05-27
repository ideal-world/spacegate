#!/usr/bin/env bash
set -euo pipefail

BASE_DIR="${SPACEGATE_OTEL_DIR:-/tmp/spacegate-otel}"
CONFIG_DIR="$BASE_DIR/config"

if [ ! -f "$CONFIG_DIR/config.json" ]; then
  "$(dirname "$0")/prepare-config.sh"
fi

RUST_LOG="${RUST_LOG:-info}" cargo run -p spacegate --features fs,wasm,static-openssl -- \
  -c "file:$CONFIG_DIR"
