#!/usr/bin/env bash
# 构建 K8s 所需镜像：ai-gateway-service + SpaceGate（k8s 模式）
set -euo pipefail
DIR="$(cd "$(dirname "$0")" && pwd)"
SG_ROOT="$(cd "$DIR/../../.." && pwd)"
HAI_HUB_ROOT="${HAI_HUB_ROOT:-$(cd "$SG_ROOT/../hai-hub" 2>/dev/null && pwd || true)}"

SERVICE_IMAGE="${AI_GATEWAY_SERVICE_IMAGE:-ai-gateway/service:dev}"
SG_IMAGE="${SPACEGATE_K8S_IMAGE:-ai-gateway/spacegate:k8s}"

echo "==> 构建 ai-gateway-service"
docker build -f "$SG_ROOT/resource/docker/ai-gateway-service/Dockerfile" \
  -t "$SERVICE_IMAGE" \
  "$SG_ROOT"

if [[ -z "$HAI_HUB_ROOT" || ! -f "$HAI_HUB_ROOT/backend/hai-hub-spacegate-plugins/Cargo.toml" ]]; then
  echo "ERROR: HAI_HUB_ROOT must point to the hai-hub repo." >&2
  exit 1
fi

echo "==> 构建 SpaceGate（wasm + dylib + k8s）"
docker build --build-context "hai_hub=$HAI_HUB_ROOT" \
  -f "$SG_ROOT/resource/docker/spacegate-k8s/Dockerfile" \
  -t "$SG_IMAGE" \
  "$SG_ROOT"

echo "Done."
echo "  ai-gateway-service: $SERVICE_IMAGE"
echo "  spacegate (k8s):    $SG_IMAGE"
echo ""
echo "更新 DaemonSet 镜像（若已安装 SpaceGate）："
echo "  kubectl set image daemonset/spacegate spacegate=$SG_IMAGE -n spacegate"
