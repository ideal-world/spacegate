#!/usr/bin/env bash
# Build the current SpaceGate code and deploy it to the Kubernetes test namespace.
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/../../.." && pwd)"
NAMESPACE="${NAMESPACE:-spacegate}"
IMAGE="${SPACEGATE_IMAGE:-spacegate:test}"
DOCKERFILE="$ROOT/resource/docker/spacegate-k8s/Dockerfile"
TIMEOUT="${ROLLOUT_TIMEOUT:-180s}"
HAI_HUB_ROOT="${HAI_HUB_ROOT:-$(cd "$ROOT/../hai-hub" 2>/dev/null && pwd || true)}"

APPLY_GATEWAY_API="${APPLY_GATEWAY_API:-true}"
APPLY_WASMPLUGIN_CRD="${APPLY_WASMPLUGIN_CRD:-true}"
APPLY_OTEL_STACK="${APPLY_OTEL_STACK:-true}"
APPLY_SAMPLE_GATEWAY_CONFIG="${APPLY_SAMPLE_GATEWAY_CONFIG:-true}"
BUILD_IMAGE="${BUILD_IMAGE:-true}"
LOAD_KIND="${LOAD_KIND:-false}"
KIND_CLUSTER="${KIND_CLUSTER:-kind}"
LOAD_K3D="${LOAD_K3D:-false}"
K3D_CLUSTER="${K3D_CLUSTER:-}"
GATEWAY_NAME="${GATEWAY_NAME:-spacegate-test}"
GATEWAY_PORT="${GATEWAY_PORT:-9993}"
OTEL_ENABLED="${OTEL_ENABLED:-true}"
OTEL_SERVICE_NAME="${OTEL_SERVICE_NAME:-spacegate-test}"
OTEL_ENDPOINT="${OTEL_ENDPOINT:-http://spacegate-otel-collector.${NAMESPACE}.svc.cluster.local:4317}"
OTEL_PROTOCOL="${OTEL_PROTOCOL:-grpc}"
OTEL_TRACES_ENABLED="${OTEL_TRACES_ENABLED:-true}"
OTEL_TRACES_SAMPLE_RATIO="${OTEL_TRACES_SAMPLE_RATIO:-1.0}"
OTEL_METRICS_ENABLED="${OTEL_METRICS_ENABLED:-true}"
OTEL_METRICS_EXPORT_INTERVAL_MS="${OTEL_METRICS_EXPORT_INTERVAL_MS:-60000}"
OTEL_LOGS_ENABLED="${OTEL_LOGS_ENABLED:-true}"
OTEL_LOGS_LEVEL="${OTEL_LOGS_LEVEL:-info}"

require_cmd() {
  command -v "$1" >/dev/null 2>&1 || {
    echo "ERROR: missing command: $1" >&2
    exit 1
  }
}

if [[ "$NAMESPACE" != "spacegate" ]]; then
  echo "ERROR: this script uses the existing manifests, which are hard-coded for namespace 'spacegate'." >&2
  echo "       Set NAMESPACE=spacegate or create a separate kustomize overlay first." >&2
  exit 1
fi

require_cmd kubectl
if [[ "$BUILD_IMAGE" == "true" ]]; then
  require_cmd docker
  if [[ -z "$HAI_HUB_ROOT" || ! -f "$HAI_HUB_ROOT/backend/hai-hub-spacegate-plugins/Cargo.toml" ]]; then
    echo "ERROR: HAI_HUB_ROOT must point to the hai-hub repo containing backend/hai-hub-spacegate-plugins." >&2
    echo "       Current HAI_HUB_ROOT='$HAI_HUB_ROOT'" >&2
    exit 1
  fi
fi

cd "$ROOT"

echo "==> Checking HAI built-in plugin references"
if command -v rg >/dev/null 2>&1; then
  if rg -n 'plugin-hai|spacegate-plugin/hai|feature = "hai"|cfg\(.*hai|plugins::hai|pub mod hai|hai = \[' crates/plugin crates/shell binary Cargo.toml --glob '!target/**'; then
    echo "ERROR: HAI built-in plugin references still exist. Remove them before deploying this test build." >&2
    exit 1
  fi
else
  echo "    rg not found; skipping HAI reference check"
fi

if [[ "$BUILD_IMAGE" == "true" ]]; then
  echo "==> Building SpaceGate image: $IMAGE"
  docker build \
    --build-context "hai_hub=$HAI_HUB_ROOT" \
    -f "$DOCKERFILE" \
    -t "$IMAGE" \
    "$ROOT"

  if [[ "$LOAD_KIND" == "true" ]]; then
    require_cmd kind
    echo "==> Loading image into kind cluster: $KIND_CLUSTER"
    kind load docker-image "$IMAGE" --name "$KIND_CLUSTER"
  fi

  if [[ "$LOAD_K3D" == "true" ]]; then
    require_cmd k3d
    if [[ -z "$K3D_CLUSTER" ]]; then
      echo "ERROR: set K3D_CLUSTER when LOAD_K3D=true" >&2
      exit 1
    fi
    echo "==> Importing image into k3d cluster: $K3D_CLUSTER"
    k3d image import "$IMAGE" -c "$K3D_CLUSTER"
  fi
else
  echo "==> Skipping image build; using existing image: $IMAGE"
fi

echo "==> Applying Kubernetes prerequisites"
if [[ "$APPLY_GATEWAY_API" == "true" ]]; then
  kubectl apply -f "$ROOT/resource/kube-manifests/gateway-api-0.6.2-experimental-china.yaml"
fi
kubectl apply -f "$ROOT/resource/kube-manifests/namespace.yaml"
kubectl apply -f "$ROOT/resource/kube-manifests/gatewayclass.yaml"
kubectl apply -f "$ROOT/resource/kube-manifests/spacegate-httproute.yaml"
kubectl apply -f "$ROOT/resource/kube-manifests/spacegate-mcproute.yaml"
if [[ "$APPLY_WASMPLUGIN_CRD" == "true" ]]; then
  kubectl apply -f "$ROOT/resource/kube-manifests/higress-wasmplugin-crd.yaml"
fi

echo "==> Applying SpaceGate controller resources"
kubectl apply -f "$ROOT/resource/kube-manifests/spacegate-gateway.yaml"
kubectl apply -f "$ROOT/deploy/k8s/ai-gateway/spacegate-rbac-cluster.yaml"

if [[ "$APPLY_OTEL_STACK" == "true" ]]; then
  echo "==> Applying OTEL Collector + ClickHouse"
  kubectl apply -n "$NAMESPACE" -f "$ROOT/deploy/k8s/test-spacegate/otel-stack.yaml"
  kubectl rollout status deployment/spacegate-clickhouse -n "$NAMESPACE" --timeout="$TIMEOUT"
  kubectl rollout status deployment/spacegate-otel-collector -n "$NAMESPACE" --timeout="$TIMEOUT"
fi

echo "==> Updating DaemonSet image"
kubectl set image daemonset/spacegate spacegate="$IMAGE" -n "$NAMESPACE"
kubectl set env daemonset/spacegate -n "$NAMESPACE" \
  SPACEGATE_OTEL_ENABLED="$OTEL_ENABLED" \
  SPACEGATE_OTEL_SERVICE_NAME="$OTEL_SERVICE_NAME" \
  SPACEGATE_OTEL_ENDPOINT="$OTEL_ENDPOINT" \
  SPACEGATE_OTEL_PROTOCOL="$OTEL_PROTOCOL" \
  SPACEGATE_OTEL_TRACES_ENABLED="$OTEL_TRACES_ENABLED" \
  SPACEGATE_OTEL_TRACES_SAMPLE_RATIO="$OTEL_TRACES_SAMPLE_RATIO" \
  SPACEGATE_OTEL_METRICS_ENABLED="$OTEL_METRICS_ENABLED" \
  SPACEGATE_OTEL_METRICS_EXPORT_INTERVAL_MS="$OTEL_METRICS_EXPORT_INTERVAL_MS" \
  SPACEGATE_OTEL_LOGS_ENABLED="$OTEL_LOGS_ENABLED" \
  SPACEGATE_OTEL_LOGS_LEVEL="$OTEL_LOGS_LEVEL"
kubectl rollout status daemonset/spacegate -n "$NAMESPACE" --timeout="$TIMEOUT"

if [[ "$APPLY_SAMPLE_GATEWAY_CONFIG" == "true" ]]; then
  echo "==> Applying sample Gateway / HTTPRoute / upstream"
  kubectl apply -n "$NAMESPACE" -f - <<YAML
apiVersion: apps/v1
kind: Deployment
metadata:
  name: spacegate-test-upstream
  labels:
    app: spacegate-test-upstream
spec:
  replicas: 1
  selector:
    matchLabels:
      app: spacegate-test-upstream
  template:
    metadata:
      labels:
        app: spacegate-test-upstream
    spec:
      containers:
        - name: echo
          image: hashicorp/http-echo:1.0.0
          args:
            - -text={"ok":true,"source":"spacegate-test-upstream"}
            - -listen=:9000
          ports:
            - name: http
              containerPort: 9000
---
apiVersion: v1
kind: Service
metadata:
  name: spacegate-test-upstream
spec:
  selector:
    app: spacegate-test-upstream
  ports:
    - name: http
      port: 9000
      targetPort: http
---
apiVersion: gateway.networking.k8s.io/v1beta1
kind: Gateway
metadata:
  name: ${GATEWAY_NAME}
  annotations:
    enable_x_request_id: "true"
spec:
  gatewayClassName: spacegate
  listeners:
    - name: http
      port: ${GATEWAY_PORT}
      protocol: HTTP
      allowedRoutes:
        namespaces:
          from: Same
---
apiVersion: gateway.networking.k8s.io/v1beta1
kind: HTTPRoute
metadata:
  name: spacegate-test-route
spec:
  parentRefs:
    - name: ${GATEWAY_NAME}
      namespace: ${NAMESPACE}
  rules:
    - matches:
        - path:
            type: PathPrefix
            value: /
      backendRefs:
        - name: spacegate-test-upstream
          port: 9000
YAML
fi

echo "==> Current SpaceGate pods"
kubectl get pods -n "$NAMESPACE" -l app=spacegate -o wide

echo ""
echo "Deployment finished."
echo "Useful checks:"
echo "  kubectl logs -n $NAMESPACE -l app=spacegate --tail=100"
echo "  kubectl logs -n $NAMESPACE deployment/spacegate-otel-collector --tail=100"
echo "  kubectl get gatewayclass,gateway,httproute -A"
echo "  kubectl get sgfilter,wasmplugin -n $NAMESPACE"
echo "  curl -i http://<node-ip>:$GATEWAY_PORT/"
echo "  kubectl exec -n $NAMESPACE deploy/spacegate-clickhouse -- clickhouse-client --database otel --query \"SELECT 'otel_logs', count() FROM otel_logs UNION ALL SELECT 'otel_traces', count() FROM otel_traces\""
