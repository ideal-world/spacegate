#!/usr/bin/env bash
# 构建 ai-gateway-service Linux 镜像（供 K8s 使用）
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/../../.." && pwd)"
cd "$ROOT"

IMAGE="${AI_GATEWAY_SERVICE_IMAGE:-ai-gateway/service:dev}"
DOCKERFILE="$(dirname "$0")/docker/Dockerfile.ai-gateway-service"

echo "Building ai-gateway-service (linux/amd64) ..."
docker build -f "$DOCKERFILE" \
  --build-arg SPACEGATE_ROOT="$ROOT" \
  -t "$IMAGE" \
  "$ROOT"

echo "Done. Image: $IMAGE"
echo "For k3d/minikube: import locally, e.g."
echo "  k3d image import $IMAGE -c <cluster>"
