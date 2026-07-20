#!/usr/bin/env bash
# 部署 AI Gateway K8s 基础设施（不含默认 HTTPRoute ai-api / SgFilter）。
# ai-gateway-queue Wasm 已内置在 SpaceGate 镜像中。
set -euo pipefail
DIR="$(cd "$(dirname "$0")" && pwd)"
ROOT="$(cd "$DIR/../../.." && pwd)"
KUSTOMIZE_FILE="$DIR/kustomization-infra.yaml"

echo "==> 检查 SpaceGate 前置（namespace / GatewayClass / DaemonSet）"
if ! kubectl get namespace spacegate >/dev/null 2>&1; then
  echo "ERROR: namespace 'spacegate' 不存在。请先执行：" >&2
  echo "  ./scripts/deploy.sh k8s install-prereq" >&2
  echo "  或 kubectl apply -f $ROOT/resource/kube-manifests/" >&2
  exit 1
fi

echo "==> 移除默认 Demo 路由（若存在）"
kubectl delete -f "$DIR/httproute-ai.yaml" -n spacegate --ignore-not-found
kubectl delete -f "$DIR/sgfilter-ai-gateway-queue.yaml" -n spacegate --ignore-not-found

echo "==> 应用 Kustomize（infra-only，无 ai-api HTTPRoute）"
KUST_BACKUP="$DIR/kustomization.yaml.full.bak"
cp "$DIR/kustomization.yaml" "$KUST_BACKUP"
cp "$DIR/kustomization-infra.yaml" "$DIR/kustomization.yaml"
kubectl apply -k "$DIR"
mv "$KUST_BACKUP" "$DIR/kustomization.yaml"

echo "==> 确保 SpaceGate DaemonSet 使用 K8s 模式本地镜像"
SG_IMAGE="${SPACEGATE_K8S_IMAGE:-ai-gateway/spacegate:k8s}"
if kubectl get daemonset spacegate -n spacegate >/dev/null 2>&1; then
  kubectl set image daemonset/spacegate spacegate="$SG_IMAGE" -n spacegate
  kubectl rollout status daemonset/spacegate -n spacegate --timeout=180s
fi

echo "==> 等待 AI Gateway Pod Ready"
kubectl wait --for=condition=ready pod \
  -l 'app.kubernetes.io/name in (ai-gateway-redis,ai-gateway-service,ai-gateway-mock-upstream)' \
  -n spacegate \
  --timeout=180s

echo ""
echo "部署完成（无默认 ai-api 路由）。"
echo "  验证: $DIR/verify-infra.sh"
echo ""
echo "后续：在管理界面或 kubectl 自行创建 HTTPRoute，并挂载 SgFilter / Wasm 插件。"
echo "  Gateway 入口: ai-gateway（:9993，SpaceGate hostNetwork）"
