#!/usr/bin/env bash
# 编译 ai-gateway-queue Wasm 并推送到 OCI 仓库（需 oras + 仓库登录）
set -euo pipefail

REGISTRY="${REGISTRY:?请设置 REGISTRY，例如 ghcr.io/your-org}"
TAG="${TAG:-v1.0.0}"
IMAGE="${IMAGE:-ai-gateway-queue}"
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
WASM="$ROOT/plugins/wasm/target/wasm32-wasip1/release/spacegate_plugin_ai_gateway_queue.wasm"

if ! command -v oras >/dev/null 2>&1; then
  echo "ERROR: 未找到 oras。安装: brew install oras" >&2
  exit 1
fi

echo "==> 编译 Wasm"
cd "$ROOT"
rustup target add wasm32-wasip1 2>/dev/null || true
cargo build --release \
  --target wasm32-wasip1 \
  --manifest-path plugins/wasm/Cargo.toml \
  -p spacegate_plugin_ai_gateway_queue

DIGEST=$(shasum -a 256 "$WASM" | awk '{print $1}')
REF="${REGISTRY}/${IMAGE}:${TAG}"

echo "==> 推送到 ${REF}"
oras push "$REF" \
  --artifact-type application/vnd.module.wasm.content.layer.v1+wasm \
  "${WASM}:application/wasm"

echo ""
echo "推送完成。在 SgFilter / WasmPlugin 中使用："
echo "  url:    oci://${REF}"
echo "  sha256: sha256:${DIGEST}"
