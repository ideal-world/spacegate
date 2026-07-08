#!/usr/bin/env bash
# Wasm 策略纯逻辑 host 侧单测（绕过 wasm32 默认 target）
set -euo pipefail
DIR="$(cd "$(dirname "$0")/../../../plugins/wasm/ai-gateway-queue" && pwd)"
HOST=$(rustc -vV | sed -n 's/host: //p')
cd "$DIR"
cargo test --lib --target "$HOST" "$@"
