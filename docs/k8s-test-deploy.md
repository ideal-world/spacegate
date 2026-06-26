# SpaceGate K8S 测试环境部署教程

本文档用于把当前仓库代码部署到 K8S 测试环境。当前前提是：`hai-*` 系列插件已经迁移到 `hai-hub`，SpaceGate 本仓库不再内置这些插件源码；测试镜像会从 `hai-hub` 构建外部 dylib，并在 SpaceGate 启动时加载注册。

## 部署目标

- 构建当前 SpaceGate 代码为 K8S 测试镜像。
- 从 `hai-hub` 构建 `hai-hub-spacegate-plugins` 动态库，并放入镜像 `/lib/spacegate/plugins/`。
- 安装或更新 `spacegate` namespace、Gateway API CRD、SpaceGate CRD/RBAC/DaemonSet。
- 将 SpaceGate DaemonSet 镜像切换为本次构建的测试镜像。
- 创建一组测试 Gateway、HTTPRoute 和 mock upstream，用于验证网关配置已被 SpaceGate K8S listener 接收。
- 保留 SpaceGate 自身 K8S 配置监听、Gateway/HTTPRoute、SgFilter、WasmPlugin、OTLP traces/metrics/logs 能力。
- 不再把 HAI 作为 SpaceGate 内置插件部署。HAI 由 `hai-hub` 独立维护，测试镜像通过 dylib 加载注册。

## 前置条件

本机需要：

- `kubectl` 已指向测试集群。
- `docker` 可用，用于构建测试镜像。
- 若是 `kind` 集群，安装 `kind`。
- 若是 `k3d` 集群，安装 `k3d`。
- 当前仓库已完成 HAI 内置插件清理，并且 `cargo build` 本地可通过。

确认当前集群：

```bash
kubectl config current-context
kubectl get nodes
```

## 一键部署

默认构建镜像 `spacegate:test`，部署到 `spacegate` namespace：

```bash
cd /Users/yiye/projectSpace/huayun_project/spacegate
./deploy/k8s/test-spacegate/deploy.sh
```

脚本默认按当前仓库旁边的 `../hai-hub` 查找插件项目。如果目录不同，显式指定：

```bash
HAI_HUB_ROOT=/path/to/hai-hub ./deploy/k8s/test-spacegate/deploy.sh
```

如果是 `kind` 测试集群，需要把本地镜像导入集群：

```bash
LOAD_KIND=true KIND_CLUSTER=kind ./deploy/k8s/test-spacegate/deploy.sh
```

如果是 `k3d` 测试集群：

```bash
LOAD_K3D=true K3D_CLUSTER=<cluster-name> ./deploy/k8s/test-spacegate/deploy.sh
```

如果镜像已由 CI 构建并推送到测试镜像仓库，可以跳过本地构建：

```bash
BUILD_IMAGE=false SPACEGATE_IMAGE=registry.example.com/spacegate:test-20260527 ./deploy/k8s/test-spacegate/deploy.sh
```

默认会一起部署 OpenTelemetry Collector 和 ClickHouse，并启用 OTLP 三信号。默认 endpoint 是集群内 Service：

```bash
http://spacegate-otel-collector.spacegate.svc.cluster.local:4317
```

如果测试环境已经有统一 Collector，可以跳过内置 OTEL 栈并指定外部 endpoint：

```bash
APPLY_OTEL_STACK=false \
OTEL_ENABLED=true \
OTEL_ENDPOINT=http://otel-collector.observability.svc.cluster.local:4317 \
OTEL_PROTOCOL=grpc \
./deploy/k8s/test-spacegate/deploy.sh
```

如果不想启用 OTLP：

```bash
APPLY_OTEL_STACK=false OTEL_ENABLED=false ./deploy/k8s/test-spacegate/deploy.sh
```

## 可配置环境变量

| 变量 | 默认值 | 说明 |
|------|--------|------|
| `SPACEGATE_IMAGE` | `spacegate:test` | 部署到 DaemonSet 的 SpaceGate 镜像 |
| `HAI_HUB_ROOT` | `../hai-hub` | `hai-hub` 仓库路径，用于构建 HAI dylib 插件 |
| `BUILD_IMAGE` | `true` | 是否本地执行 `docker build` |
| `LOAD_KIND` | `false` | 是否导入镜像到 kind |
| `KIND_CLUSTER` | `kind` | kind 集群名 |
| `LOAD_K3D` | `false` | 是否导入镜像到 k3d |
| `K3D_CLUSTER` | 空 | k3d 集群名 |
| `APPLY_GATEWAY_API` | `true` | 是否应用仓库内 Gateway API CRD |
| `APPLY_WASMPLUGIN_CRD` | `true` | 是否应用 Higress WasmPlugin CRD |
| `APPLY_OTEL_STACK` | `true` | 是否部署测试用 Collector + ClickHouse |
| `APPLY_SAMPLE_GATEWAY_CONFIG` | `true` | 是否创建测试 Gateway/HTTPRoute/mock upstream |
| `ROLLOUT_TIMEOUT` | `180s` | DaemonSet rollout 等待时间 |
| `GATEWAY_NAME` | `spacegate-test` | 测试 Gateway 名称 |
| `GATEWAY_PORT` | `9993` | 测试 Gateway 监听端口，SpaceGate DaemonSet 使用 hostNetwork |
| `OTEL_ENABLED` | `true` | 是否启用 OTLP 初始化 |
| `OTEL_SERVICE_NAME` | `spacegate-test` | OTLP resource service name |
| `OTEL_ENDPOINT` | `http://spacegate-otel-collector.spacegate.svc.cluster.local:4317` | OTLP Collector 地址 |
| `OTEL_PROTOCOL` | `grpc` | OTLP 协议，支持 `grpc` / `http` |
| `OTEL_TRACES_ENABLED` | `true` | 是否导出 traces |
| `OTEL_TRACES_SAMPLE_RATIO` | `1.0` | trace 采样率 |
| `OTEL_METRICS_ENABLED` | `true` | 是否导出 metrics |
| `OTEL_METRICS_EXPORT_INTERVAL_MS` | `60000` | metrics 导出间隔 |
| `OTEL_LOGS_ENABLED` | `true` | 是否导出 logs |
| `OTEL_LOGS_LEVEL` | `info` | OTLP logs 级别 |

当前脚本复用仓库已有 manifest，这些 manifest 固定使用 `spacegate` namespace；如需其他 namespace，请先做 kustomize overlay，不要直接替换脚本变量。

## 脚本做了什么

`deploy/k8s/test-spacegate/deploy.sh` 会按顺序执行：

1. 检查 `kubectl`、`docker` 等命令。
2. 使用 `rg` 检查 SpaceGate 内置侧是否仍残留 HAI feature/注册引用。
3. 使用 `deploy/k8s/test-spacegate/Dockerfile.spacegate` 构建当前代码镜像：SpaceGate 编译启用 `build-k8s,wasm,dylib,static-openssl`；`hai-hub-spacegate-plugins` 在同一镜像构建中编译为 Linux `.so`。
4. 可选导入镜像到 `kind` 或 `k3d`。
5. 应用：
   - `resource/kube-manifests/gateway-api-0.6.2-experimental-china.yaml`
   - `resource/kube-manifests/namespace.yaml`
   - `resource/kube-manifests/gatewayclass.yaml`
   - `resource/kube-manifests/higress-wasmplugin-crd.yaml`
   - `resource/kube-manifests/spacegate-gateway.yaml`
   - `deploy/k8s/ai-gateway/spacegate-rbac-cluster.yaml`
6. 默认应用 `deploy/k8s/test-spacegate/otel-stack.yaml`，部署 ClickHouse、Collector ConfigMap、Collector Deployment/Service。
7. 更新 `daemonset/spacegate` 镜像并等待 rollout。
8. 向 `daemonset/spacegate` 写入 `SPACEGATE_OTEL_*` 环境变量，用于控制 OTLP exporter。
9. 默认创建测试 Gateway、HTTPRoute、mock upstream。Gateway 上会写入 `enable_x_request_id` annotation。

## 网关配置如何引入

K8S 模式下 SpaceGate 使用启动参数 `-c k8s:spacegate`，监听 `spacegate` namespace 下的 Gateway API 资源：

- `GatewayClass`：选择 `controllerName: spacegate.idealworld.group/spacegate-controller`
- `Gateway`：声明监听端口、协议和 SpaceGate 参数
- `HTTPRoute`：声明路径匹配和后端 Service
- `SgFilter` / `WasmPlugin`：按需挂载外部插件

一键脚本默认创建以下测试资源：

```yaml
apiVersion: gateway.networking.k8s.io/v1beta1
kind: Gateway
metadata:
  name: spacegate-test
  annotations:
    enable_x_request_id: "true"
spec:
  gatewayClassName: spacegate
  listeners:
    - name: http
      port: 9993
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
    - name: spacegate-test
      namespace: spacegate
  rules:
    - matches:
        - path:
            type: PathPrefix
            value: /
      backendRefs:
        - name: spacegate-test-upstream
          port: 9000
```

部署后访问：

```bash
curl -i http://<node-ip>:9993/
```

如果你要接入 `hai-hub`，不要恢复 SpaceGate 内置 `hai-*` 插件；应由 `hai-hub` 提供自己的 Gateway/HTTPRoute/SgFilter/WasmPlugin manifest，或挂载到上述 Gateway/HTTPRoute。

## HAI 插件如何加载

当前测试镜像通过 SpaceGate 的 native dylib 路径加载 HAI 插件：

- `binary/spacegate` 启用 `dylib` feature 后，会扫描启动参数 `--plugins/-p` 指向的目录。
- Linux 镜像中默认插件目录是 `/lib/spacegate/plugins`。
- Dockerfile 会从 `HAI_HUB_ROOT` 对应的仓库构建 `hai-hub-spacegate-plugins`，并复制 `libhai_hub_spacegate_plugins.so` 到该目录。
- 该 dylib 暴露 `register(repo: &PluginRepository)`，一次性注册 `hub-request-id`、`hai-observe`、`hai-auth`、`hai-asset`、`hai-quota`、`hai-dispatch`。

注意：动态库只在 SpaceGate 进程启动时扫描加载。更新 HAI 插件代码后，需要重新构建镜像并滚动重启 DaemonSet。

## OTLP 配置如何引入

OTLP exporter 是进程级能力，当前 K8S 部署脚本通过 DaemonSet 环境变量注入，而不是通过 Gateway/HTTPRoute 注入。脚本默认先部署测试用 Collector 和 ClickHouse：

- Collector Service：`spacegate-otel-collector.spacegate.svc.cluster.local`
- OTLP gRPC：`4317`
- OTLP HTTP：`4318`
- ClickHouse Service：`spacegate-clickhouse.spacegate.svc.cluster.local`
- ClickHouse HTTP：`8123`
- ClickHouse Native：`9000`
- ClickHouse database：`otel`

Collector 配置来自 [otel-stack.yaml](/Users/yiye/projectSpace/huayun_project/spacegate/deploy/k8s/test-spacegate/otel-stack.yaml)，ClickHouse exporter 使用：

```yaml
endpoint: tcp://spacegate-clickhouse:9000?dial_timeout=10s
database: otel
logs_table_name: otel_logs
traces_table_name: otel_traces
metrics_tables:
  sum:
    name: otel_metrics_sum
  histogram:
    name: otel_metrics_histogram
```

脚本会把 `OTEL_*` 参数转换为容器内的 `SPACEGATE_OTEL_*` 环境变量：

```yaml
env:
  - name: SPACEGATE_OTEL_ENABLED
    value: "true"
  - name: SPACEGATE_OTEL_SERVICE_NAME
    value: "spacegate-test"
  - name: SPACEGATE_OTEL_ENDPOINT
    value: "http://spacegate-otel-collector.spacegate.svc.cluster.local:4317"
  - name: SPACEGATE_OTEL_PROTOCOL
    value: "grpc"
  - name: SPACEGATE_OTEL_TRACES_ENABLED
    value: "true"
  - name: SPACEGATE_OTEL_TRACES_SAMPLE_RATIO
    value: "1.0"
  - name: SPACEGATE_OTEL_METRICS_ENABLED
    value: "true"
  - name: SPACEGATE_OTEL_METRICS_EXPORT_INTERVAL_MS
    value: "60000"
  - name: SPACEGATE_OTEL_LOGS_ENABLED
    value: "true"
  - name: SPACEGATE_OTEL_LOGS_LEVEL
    value: "info"
```

代码路径是：

- `crates/shell/src/config.rs` 在 `startup_with_shutdown_signal` 中调用 `crate::observability::init(&init_config.observability)`。
- `crates/shell/src/observability.rs` 会读取 `SPACEGATE_OTEL_*` 环境变量覆盖默认 `ObservabilityConfig`。
- `crates/shell/src/observability.rs` 根据最终配置初始化 OTLP traces、metrics、logs exporter。

Gateway annotations 中仍保留了 `enable_x_request_id` 这类网关级参数；这类参数由 `crates/config/src/service/k8s/convert/gateway_k8s_conv.rs` 解析到 `SgGateway.parameters`，并在网关实例创建时生效。OTLP exporter 是进程级初始化，改动 `SPACEGATE_OTEL_*` 后需要 DaemonSet rollout。

## 部署后验证

查看 DaemonSet：

```bash
kubectl get daemonset spacegate -n spacegate
kubectl get pods -n spacegate -l app=spacegate -o wide
```

查看日志：

```bash
kubectl logs -n spacegate -l app=spacegate --tail=100
kubectl logs -n spacegate deployment/spacegate-otel-collector --tail=100
```

确认 Gateway API 和 SpaceGate CRD：

```bash
kubectl get gatewayclass
kubectl get gateway -A
kubectl get httproute -A
kubectl get sgfilter -n spacegate
kubectl get wasmplugin -n spacegate
```

验证测试 Gateway：

```bash
kubectl get gateway spacegate-test -n spacegate -o yaml
kubectl get httproute spacegate-test-route -n spacegate -o yaml
curl -i http://<node-ip>:9993/
```

验证 OTLP 环境变量：

```bash
kubectl get daemonset spacegate -n spacegate \
  -o jsonpath='{.spec.template.spec.containers[?(@.name=="spacegate")].env}{"\n"}'
```

验证 ClickHouse 入库：

```bash
kubectl exec -n spacegate deploy/spacegate-clickhouse -- \
  clickhouse-client --database otel --query "
    SELECT 'otel_logs' AS table, count() FROM otel_logs
    UNION ALL SELECT 'otel_traces', count() FROM otel_traces
    UNION ALL SELECT 'otel_metrics_sum', count() FROM otel_metrics_sum
    UNION ALL SELECT 'otel_metrics_histogram', count() FROM otel_metrics_histogram
  "
```

如果表还不存在，先访问一次测试网关并等待 Collector 创建 schema：

```bash
curl -i http://<node-ip>:9993/
sleep 10
```

确认 HAI 不再是 SpaceGate 内置插件：

```bash
rg -n 'plugin-hai|spacegate-plugin/hai|feature = "hai"|plugins::hai|pub mod hai|hai-observe|hai-auth|hai-asset|hai-quota|hai-dispatch' \
  crates/plugin crates/shell binary Cargo.toml
```

该命令不应再命中 SpaceGate 内置 feature 或注册路径。若 `hai-hub` 以外部 WasmPlugin/SgFilter 方式注册 HAI 能力，相关配置应位于 `hai-hub` 的部署仓库或测试环境资源中，而不是 SpaceGate 内置插件列表里。

## 部署业务路由

SpaceGate DaemonSet 启动后只负责监听 K8S 资源。测试业务流量还需要 Gateway、HTTPRoute，以及按需挂载的 SgFilter/WasmPlugin。

可以参考现有 AI Gateway 示例：

```bash
kubectl apply -k deploy/k8s/ai-gateway
```

如果 `hai-hub` 已提供自己的 K8S manifest，应在 SpaceGate 基础部署完成后应用 `hai-hub` 侧的 Gateway/HTTPRoute/SgFilter/WasmPlugin 资源。

## 回滚

切回已有镜像：

```bash
kubectl set image daemonset/spacegate spacegate=<previous-image> -n spacegate
kubectl rollout status daemonset/spacegate -n spacegate --timeout=180s
```

查看 rollout 历史：

```bash
kubectl rollout history daemonset/spacegate -n spacegate
```

## 常见问题

### 镜像构建很慢

SpaceGate K8S 镜像会在 Docker build 内编译 Rust workspace，首次构建较慢。测试环境可由 CI 构建并推送镜像，然后使用：

```bash
BUILD_IMAGE=false SPACEGATE_IMAGE=<registry>/<spacegate>:<tag> ./deploy/k8s/test-spacegate/deploy.sh
```

### Pod 一直 `ImagePullBackOff`

本地 `kind`/`k3d` 集群默认拉不到宿主机本地镜像。使用：

```bash
LOAD_KIND=true KIND_CLUSTER=<cluster> ./deploy/k8s/test-spacegate/deploy.sh
```

或：

```bash
LOAD_K3D=true K3D_CLUSTER=<cluster> ./deploy/k8s/test-spacegate/deploy.sh
```

### SpaceGate 启动后没有路由

检查是否已经创建 Gateway/HTTPRoute：

```bash
kubectl get gateway,httproute -A
```

SpaceGate 基础部署只安装控制器和 CRD，不会自动创建你的业务路由。

### HAI 请求不再生效

这是预期行为。SpaceGate 不再内置注册 `hai-*` 插件。请确认 `hai-hub` 已部署对应的外部插件/过滤器资源，并且目标 Gateway 或 HTTPRoute 已挂载该资源。
