#!/usr/bin/env bash
# 构建 K8s 所需镜像：ai-gateway-service + SpaceGate（k8s 模式）
set -euo pipefail
DIR="$(cd "$(dirname "$0")" && pwd)"
SG_ROOT="$(cd "$DIR/../../.." && pwd)"
# ai-gateway-dev 工作区根（spacegate 的父目录）
WORKSPACE_ROOT="$(cd "$SG_ROOT/.." && pwd)"

SERVICE_IMAGE="${AI_GATEWAY_SERVICE_IMAGE:-ai-gateway/service:dev}"
SG_IMAGE="${SPACEGATE_K8S_IMAGE:-ai-gateway/spacegate:k8s}"

echo "==> 构建 ai-gateway-service"
docker build -f "$DIR/docker/Dockerfile.ai-gateway-service" \
  --build-arg SPACEGATE_ROOT="$SG_ROOT" \
  -t "$SERVICE_IMAGE" \
  "$SG_ROOT"

echo "==> 构建 SpaceGate（wasm + axum + k8s）"
docker build -f "$WORKSPACE_ROOT/docker/Dockerfile.spacegate-k8s" \
  -t "$SG_IMAGE" \
  "$SG_ROOT"

WEB_IMAGE="${AI_GATEWAY_WEB_IMAGE:-ai-gateway/web:k8s-spa}"
echo "==> 构建管理 UI（spacegate-admin-fe SPA + nginx）"
if [[ ! -f "$WORKSPACE_ROOT/spacegate-admin-fe/dist/index.html" ]]; then
  echo "    缺少 dist/index.html，尝试构建前端（需已 npm install）"
  (cd "$WORKSPACE_ROOT/spacegate-admin-fe" && VITE_AI_GATEWAY_BASE_URL=/ai-gateway npm run build) || {
    echo "    前端构建失败，请手动: cd spacegate-admin-fe && VITE_AI_GATEWAY_BASE_URL=/ai-gateway npm run build"
    exit 1
  }
fi
docker build -f "$WORKSPACE_ROOT/docker/Dockerfile.web.k8s" \
  -t "$WEB_IMAGE" \
  "$WORKSPACE_ROOT"

echo "Done."
echo "  ai-gateway-service: $SERVICE_IMAGE"
echo "  spacegate (k8s):    $SG_IMAGE"
echo "  admin web (k8s):    $WEB_IMAGE"
echo ""
echo "更新管理 UI Deployment（本地 Docker Desktop 需 Never 拉取策略）："
echo "  kubectl set image deployment/ai-gateway-web web=$WEB_IMAGE -n spacegate"
echo "  kubectl rollout status deployment/ai-gateway-web -n spacegate"
echo ""
echo "更新 DaemonSet 镜像（若已安装 SpaceGate）："
echo "  kubectl set image daemonset/spacegate spacegate=$SG_IMAGE -n spacegate"
