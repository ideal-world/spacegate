# Installation

This guide installs the current Spacegate Kubernetes Gateway, including the custom resources queried by the K8s configuration backend.

## Prerequisites

- `kubectl`
- A Spacegate image built with `build-k8s,wasm,dylib`
- A Kubernetes cluster reachable from `kubectl`

The checked-in manifests currently deploy Spacegate in the `spacegate` namespace.

## Install CRDs and the gateway

Run these commands from the `spacegate` repository root. CRDs must be installed before the Spacegate DaemonSet because the controller lists and watches them during startup.

```bash
# Gateway API v0.6.2 resources used by the current compatibility layer.
kubectl apply -f resource/kube-manifests/gateway-api-0.6.2-standard-china.yaml

# Spacegate namespace and custom resources.
kubectl apply -f resource/kube-manifests/namespace.yaml
kubectl apply -f resource/kube-manifests/spacegate-httproute.yaml
kubectl apply -f resource/kube-manifests/spacegate-mcproute.yaml
kubectl apply -f resource/kube-manifests/higress-wasmplugin-crd.yaml

# GatewayClass, RBAC and the Spacegate DaemonSet.
kubectl apply -f resource/kube-manifests/gatewayclass.yaml
kubectl apply -f resource/kube-manifests/spacegate-gateway.yaml
```

The `WasmPlugin` CRD is installed even when no WasmPlugin instance exists because the current K8s configuration backend queries that resource during initial configuration retrieval.

## Select the gateway image

Before applying `resource/kube-manifests/spacegate-gateway.yaml`, replace `registry.example.com/spacegate/spacegate:REPLACE_ME` with an immutable version or digest. The checked-in manifest deliberately has no runnable production default image.

After the DaemonSet exists, image upgrades use:

```bash
kubectl set image daemonset/spacegate \
  spacegate=<registry>/spacegate:<version> \
  -n spacegate

kubectl rollout status daemonset/spacegate -n spacegate --timeout=300s
```

The release image must be compiled with K8s, Wasm and native dylib support:

```bash
cargo build --release -p spacegate --features build-k8s,wasm,dylib
```

The base DaemonSet scans both `/lib/spacegate/plugins` for image-bundled plugins and `/var/lib/spacegate/plugins` for mounted plugins. The latter is an `emptyDir` mount in the base manifest, so it exists even when no external plugin is supplied. Mounting directly over `/lib/spacegate/plugins` can hide plugins already packaged in the image.

Wasm modules using `file:///plugins/<name>.wasm` are different from native dylibs. The base DaemonSet mounts each node's `/opt/spacegate/wasm` host directory read-only at `/plugins`; place the same verified Wasm artifact on every node before applying an SgFilter that references it.

Native dylibs are scanned only during process startup. Use an InitContainer or a volume that already contains the Linux `.so` before SpaceGate starts; adding a file to the mounted directory later requires restarting the SpaceGate Pod. Do not use a macOS `.dylib` or other non-Linux binary.

## Optional OpenTelemetry export

The base DaemonSet declares all `SPACEGATE_OTEL_*` settings but keeps OTLP disabled. To use an existing Collector:

```bash
kubectl set env daemonset/spacegate -n spacegate \
  SPACEGATE_OTEL_ENABLED=true \
  SPACEGATE_OTEL_SERVICE_NAME=spacegate-prod \
  SPACEGATE_OTEL_ENDPOINT=http://otel-collector.observability.svc.cluster.local:4317 \
  SPACEGATE_OTEL_PROTOCOL=grpc \
  SPACEGATE_OTEL_TRACES_ENABLED=true \
  SPACEGATE_OTEL_TRACES_SAMPLE_RATIO=0.1 \
  SPACEGATE_OTEL_METRICS_ENABLED=true \
  SPACEGATE_OTEL_METRICS_EXPORT_INTERVAL_MS=30000 \
  SPACEGATE_OTEL_LOGS_ENABLED=true \
  SPACEGATE_OTEL_LOGS_LEVEL=info

kubectl rollout status daemonset/spacegate -n spacegate --timeout=300s
```

`deploy/k8s/test-spacegate/otel-stack.yaml` is a test-only Collector and ClickHouse stack. Production deployments should use persistent storage, authentication, retention limits and resource constraints.

## Verify

```bash
kubectl get crd \
  httpspaceroutes.spacegate.idealworld.group \
  mcproutes.spacegate.idealworld.group \
  sgfilters.spacegate.idealworld.group \
  wasmplugins.extensions.higress.io

kubectl get pods -n spacegate -l app=spacegate
kubectl logs -n spacegate -l app=spacegate --tail=100
kubectl get gatewayclass,gateway,httproute -A
kubectl get httpspaceroute,mcproute,sgfilter,wasmplugin -n spacegate
```

For feature details, see:

- [Gateway API compatibility](gateway-api-compatibility.md)
- [MCPRoute guide](../mcp/mcp-route-guide.md)
- [K8s test deployment](test-deployment.md)
