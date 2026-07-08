#!/usr/bin/env bash
# 基础设施部署验证（不要求 HTTPRoute / 网关流量）
set -euo pipefail
NS=spacegate
pass=0
fail=0

check() {
  local name="$1" expect="$2" got="$3"
  if [[ "$got" == "$expect" ]]; then
    echo "✅ $name"
    pass=$((pass + 1))
  else
    echo "❌ $name 期望=$expect 实际=$got"
    fail=$((fail + 1))
  fi
}

echo "==> Pod 状态"
kubectl get pods -n "$NS" -l 'app.kubernetes.io/name in (ai-gateway-redis,ai-gateway-service,ai-gateway-wasm,ai-gateway-mock-upstream)'

echo "==> 不应存在默认 HTTPRoute ai-api"
if kubectl get httproute ai-api -n "$NS" >/dev/null 2>&1; then
  echo "❌ HTTPRoute ai-api 仍存在（应已删除）"
  fail=$((fail + 1))
else
  echo "✅ 无 HTTPRoute ai-api"
  pass=$((pass + 1))
fi

echo "==> 不应存在默认 SgFilter ai-gateway-queue"
if kubectl get sgfilter ai-gateway-queue -n "$NS" >/dev/null 2>&1; then
  echo "❌ SgFilter ai-gateway-queue 仍存在"
  fail=$((fail + 1))
else
  echo "✅ 无 SgFilter ai-gateway-queue"
  pass=$((pass + 1))
fi

echo "==> Gateway ai-gateway 存在"
kubectl get gateway ai-gateway -n "$NS" >/dev/null && check "Gateway ai-gateway" "ok" "ok" || fail=$((fail + 1))

echo "==> SpaceGate DaemonSet 运行中"
if kubectl get pods -n "$NS" -l app=spacegate -o jsonpath='{.items[0].status.phase}' 2>/dev/null | grep -q Running; then
  echo "✅ spacegate DaemonSet Running"
  pass=$((pass + 1))
else
  echo "❌ spacegate DaemonSet 未 Running"
  fail=$((fail + 1))
fi

echo "==> ai-gateway-service 健康（集群内 curl）"
if kubectl run curl-health-$RANDOM --rm -i --restart=Never -n "$NS" \
  --image=curlimages/curl:8.5.0 --quiet -- \
  curl -sf http://ai-gateway-service:18080/healthz >/dev/null 2>&1; then
  echo "✅ ai-gateway-service /healthz"
  pass=$((pass + 1))
else
  echo "❌ ai-gateway-service /healthz"
  fail=$((fail + 1))
fi

echo "==> Wasm HTTP 分发"
if kubectl exec -n "$NS" deploy/ai-gateway-wasm -- wget -qO- http://127.0.0.1/spacegate_plugin_ai_gateway_queue.wasm >/dev/null 2>&1; then
  echo "✅ ai-gateway-wasm 可下载 .wasm"
  pass=$((pass + 1))
else
  echo "❌ ai-gateway-wasm"
  fail=$((fail + 1))
fi

echo "=== $pass 通过, $fail 失败 ==="
exit "$fail"
