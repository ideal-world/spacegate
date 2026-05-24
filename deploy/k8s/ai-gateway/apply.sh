#!/usr/bin/env bash
# 编译 Wasm + 打包 ConfigMap + 部署 AI Gateway K8s 栈
set -euo pipefail
DIR="$(cd "$(dirname "$0")" && pwd)"
ROOT="$(cd "$DIR/../../.." && pwd)"
WASM_SRC="$ROOT/plugins/wasm/target/wasm32-wasip1/release/spacegate_plugin_ai_gateway_queue.wasm"
WASM_DST="$DIR/files/spacegate_plugin_ai_gateway_queue.wasm"

echo "==> 检查 SpaceGate 前置（namespace / GatewayClass / DaemonSet）"
if ! kubectl get namespace spacegate >/dev/null 2>&1; then
  echo "ERROR: namespace 'spacegate' 不存在。请先安装 SpaceGate：" >&2
  echo "  kubectl apply -f $ROOT/resource/kube-manifests/namespace.yaml" >&2
  echo "  kubectl apply -f $ROOT/resource/kube-manifests/gatewayclass.yaml" >&2
  echo "  kubectl apply -f $ROOT/resource/kube-manifests/spacegate-gateway.yaml" >&2
  exit 1
fi

echo "==> 编译 ai-gateway-queue Wasm"
cd "$ROOT"
rustup target add wasm32-wasip1 2>/dev/null || true
cargo build --release --target wasm32-wasip1 \
  --manifest-path plugins/wasm/Cargo.toml \
  -p spacegate_plugin_ai_gateway_queue

mkdir -p "$DIR/files"
cp "$WASM_SRC" "$WASM_DST"

echo "==> 应用 Kustomize"
kubectl apply -k "$DIR"

echo "==> 等待 Pod Ready"
kubectl wait --for=condition=ready pod \
  -l app.kubernetes.io/part-of=ai-gateway \
  -n spacegate \
  --timeout=180s

echo ""
echo "部署完成。验证："
echo "  $DIR/verify.sh"
echo ""
echo "网关入口（SpaceGate hostNetwork 监听 9993）："
echo "  curl -i http://<node-ip>:9993/v1/chat/completions \\" 
echo "    -H 'X-RateLimit-Policy: abandon' -H 'X-Tenant-Id: demo' -d '{\"prompt\":\"hi\"}'"
