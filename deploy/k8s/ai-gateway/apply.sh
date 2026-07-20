#!/usr/bin/env bash
# 部署 AI Gateway K8s 栈。ai-gateway-queue Wasm 已内置在 SpaceGate 镜像中。
set -euo pipefail
DIR="$(cd "$(dirname "$0")" && pwd)"
ROOT="$(cd "$DIR/../../.." && pwd)"

echo "==> 检查 SpaceGate 前置（namespace / GatewayClass / DaemonSet）"
if ! kubectl get namespace spacegate >/dev/null 2>&1; then
  echo "ERROR: namespace 'spacegate' 不存在。请先安装 SpaceGate：" >&2
  echo "  kubectl apply -f $ROOT/resource/kube-manifests/namespace.yaml" >&2
  echo "  kubectl apply -f $ROOT/resource/kube-manifests/gatewayclass.yaml" >&2
  echo "  kubectl apply -f $ROOT/resource/kube-manifests/spacegate-gateway.yaml" >&2
  exit 1
fi

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
