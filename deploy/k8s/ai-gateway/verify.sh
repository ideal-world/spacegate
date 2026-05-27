#!/usr/bin/env bash
# K8s 部署后冒烟验证
set -euo pipefail
DIR="$(cd "$(dirname "$0")" && pwd)"
NS=spacegate
GW="http://127.0.0.1:9993/v1/chat/completions"
PF=""

pass=0
fail=0
check() {
  local name="$1" expect="$2" got="$3"
  if [[ "$got" == "$expect" ]]; then
    echo "✅ $name ($got)"
    pass=$((pass + 1))
  else
    echo "❌ $name 期望=$expect 实际=$got"
    fail=$((fail + 1))
  fi
}

echo "==> 后端 health"
curl -sf "http://127.0.0.1:18080/healthz" >/dev/null 2>&1 \
  && echo "✅ ai-gateway-service /healthz（需 port-forward 或 hostNetwork 可达）" \
  || kubectl exec -n "$NS" deploy/ai-gateway-service -- wget -qO- http://127.0.0.1:18080/healthz >/dev/null \
  && echo "✅ ai-gateway-service /healthz（集群内）" \
  || { echo "⚠️  跳过直连 health（请 kubectl port-forward svc/ai-gateway-service 18080:18080）"; }

T="k8s-verify-$(date +%s)"
curl -sf -X PUT "http://127.0.0.1:18080/v1/admin/tenant-rate-limits" \
  -H 'Content-Type: application/json' \
  -d "{\"tenant\":\"$T\",\"rps\":5,\"burst\":5}" >/dev/null 2>&1 \
  || { kubectl port-forward -n "$NS" svc/ai-gateway-service 18080:18080 >/tmp/pf-18080.log 2>&1 & PF=$!; sleep 2; }
curl -sf -X PUT "http://127.0.0.1:18080/v1/admin/tenant-rate-limits" \
  -H 'Content-Type: application/json' \
  -d "{\"tenant\":\"$T\",\"rps\":5,\"burst\":5}" >/dev/null || true

echo "==> 网关插件 (tenant=$T)"
check "缺 Policy" 400 "$(curl -s -o /dev/null -w '%{http_code}' -X POST "$GW" -H "X-Tenant-Id: $T" -H 'Content-Type: application/json' -d '{}')"
check "abandon 配额内" 200 "$(curl -s -o /dev/null -w '%{http_code}' -X POST "$GW" -H 'X-RateLimit-Policy: abandon' -H "X-Tenant-Id: $T" -H 'Content-Type: application/json' -d '{"p":1}')"

for i in $(seq 1 10); do
  curl -s -o /dev/null -X POST "$GW" -H 'X-RateLimit-Policy: abandon' -H "X-Tenant-Id: $T" -H 'Content-Type: application/json' -d "{\"p\":$i}" || true
done
check "abandon 超额" 429 "$(curl -s -o /dev/null -w '%{http_code}' -X POST "$GW" -H 'X-RateLimit-Policy: abandon' -H "X-Tenant-Id: $T" -H 'Content-Type: application/json' -d '{"p":99}')"

kill "$PF" 2>/dev/null || true
echo "=== $pass 通过, $fail 失败 ==="
exit "$fail"
