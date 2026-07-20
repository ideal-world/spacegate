# SpaceGate 生产 K8S 部署手册

本文档用于指导运维和开发把当前工作区的 SpaceGate、SpaceGate Admin、AI Gateway 后端和 Wasm 插件按生产 K8S 方式部署。本文不使用 `Dockerfile.all-in-one` 单容器方案；生产环境必须按组件拆分部署、独立扩缩容、独立升级和回滚。

## 1. 适用范围

本手册覆盖：

- SpaceGate K8S Gateway DaemonSet。
- `spacegate-admin` 管理前后端合并镜像，包含 `spacegate-admin-fe` 静态资源和 `spacegate-admin-server` 后端。
- `ai-gateway-service` 队列、限流、wait、worker、回调服务。
- `ai-gateway-queue` Wasm 插件作为系统制品内置在 SpaceGate 镜像中。
- Redis、对象存储、OTEL Collector、ClickHouse 等外部依赖的接入要求。
- 生产部署、验证、升级、回滚和运维检查。

本手册不覆盖：

- all-in-one 单容器部署。
- 本地 Docker Compose 开发环境。
- 测试集群的一键部署脚本作为生产流水线直接使用。

## 2. 当前仓库部署资料边界

当前仓库已有以下资料可以复用：

| 资料 | 用途 | 生产使用方式 |
| --- | --- | --- |
| `docs/k8s/installation.md` | SpaceGate Gateway 基础安装 | 可作为基础安装参考，但生产建议使用固定版本 manifest 或内部镜像源 |
| `docs/k8s/test-deployment.md` | K8S 测试环境部署 | 仅供测试环境参考，不作为生产手册 |
| `deploy/README.md` | AI Gateway 插件、后端、K8S 示例、OCI Wasm 说明 | 可复用构建和配置思路，需要替换测试默认值 |
| `deploy/k8s/ai-gateway/*.yaml` | Redis、后端、前端、Gateway、HTTPRoute、SgFilter 示例 | 仅供测试和示例，禁止作为生产入口 |
| `deploy/k8s/ai-gateway/production/` | AI Gateway 生产 Kustomize 与操作手册 | AI Gateway 唯一生产部署入口 |
| `deploy/push-wasm-oci.sh` | 编译并推送 Wasm OCI 制品 | 生产推荐使用，需指定正式 registry、tag、digest |
| `resource/docker/spacegate-k8s/Dockerfile` | 构建 K8S SpaceGate 镜像并打入 HAI dylib 插件 | 可作为镜像构建参考，路径名虽是 test-spacegate，但 Dockerfile 本身包含 K8S 启动参数 |
| `resource/docker/spacegate-admin/Dockerfile` | 构建 SpaceGate Admin 前后端合并镜像 | 当前推荐的 Admin 镜像构建入口 |
| `Dockerfile.all-in-one` | 单容器打包前端、后端、网关、Redis、Nginx | 生产禁用 |

必须注意：

- `deploy/k8s/ai-gateway/build-images.sh` 仅构建 AI Gateway Service 与 K8s SpaceGate 镜像；运行时需要通过 `HAI_HUB_ROOT` 指定包含 `hai-hub-spacegate-plugins` 的 `hai-hub` 仓库。Admin 合并镜像按本手册的独立步骤构建。
- `deploy/k8s/ai-gateway/admin-ui.yaml` 是旧的 Admin Server 和 Admin Web 拆分示例，其中 `imagePullPolicy: Never`、`ai-gateway/web:k8s-spa`、`ai-gateway/admin-server:dev` 是本地/测试值。当前推荐使用 `resource/kube-manifests/spacegate-admin-server.yaml` 和 `spacegate-admin` 合并镜像。
- `deploy/k8s/ai-gateway/ai-gateway-service.yaml` 中的 `ai-gateway/service:v20260601-fix`、`AI_REQUIRE_HTTPS_CALLBACK=false` 是测试值。
- `deploy/k8s/ai-gateway/mock-upstream.yaml` 是模拟上游，生产必须替换为真实 LLM Service。
- `deploy/k8s/test-spacegate/otel-stack.yaml` 使用 `clickhouse/clickhouse-server:latest`、空密码和 `emptyDir`，只适合测试。

## 3. 生产目标拓扑

```text
Admin user
  -> Ingress / LoadBalancer
     -> /           -> spacegate-admin
     -> /api        -> spacegate-admin -> container local spacegate-admin-server
     -> /ai-gateway -> ai-gateway-queue admin APIs

External client
  -> LoadBalancer / NodePort / hostNetwork node ip
  -> SpaceGate Gateway listener
  -> HTTPRoute backend service
  -> Upstream LLM service

SpaceGate Wasm filter
  -> ai-gateway-queue
  -> Redis
  -> Object storage, optional for large body offload
  -> Callback endpoint, optional for queue policy

SpaceGate / services
  -> OTLP
  -> OpenTelemetry Collector
  -> ClickHouse or production observability backend
```

组件职责：

| 组件 | 生产职责 | 关键端口 |
| --- | --- | --- |
| SpaceGate | K8S Gateway 数据面，监听 Gateway/HTTPRoute/SgFilter/WasmPlugin | Gateway listener 端口，例如 `9993` |
| `spacegate-admin` | 管理前端 SPA + 管理 K8S 中 Gateway、HTTPRoute、SgFilter、WasmPlugin 等资源 | `9080`，容器内 Admin Server 监听 `9081` |
| `ai-gateway-queue` | Redis 队列、限流、wait、worker、回调、管理接口 | `18080` |
| Redis | 队列、令牌桶、结果、DLQ、回调重试状态 | `6379` |
| Wasm 文件 | `ai-gateway-queue` 系统插件制品 | 容器内 `/lib/spacegate/wasm/spacegate_plugin_ai_gateway_queue.wasm` |
| OTEL Collector | 接收 SpaceGate OTLP logs/traces/metrics | `4317` / `4318` |

## 4. 生产前置条件

### 4.1 K8S 集群

- `kubectl` 已指向生产集群。
- 集群能拉取生产镜像仓库中的镜像。
- 已准备 `spacegate` namespace。当前 SpaceGate K8S 部署约定运行在 `spacegate` namespace。
- 已安装 Gateway API CRD，当前仓库示例使用 v0.6.2。
- 已安装 `spacegate-httproute.yaml`、`spacegate-mcproute.yaml` 和 `higress-wasmplugin-crd.yaml`。K8S 配置后端会查询并 watch 这些资源，即使当前没有对应实例也不能省略。
- 集群节点、LB 或 Ingress 已规划业务入口端口。

### 4.2 镜像仓库和制品仓库

生产至少需要以下制品：

| 制品 | 示例 |
| --- | --- |
| SpaceGate 镜像 | `<registry>/spacegate:<version>` |
| SpaceGate Admin 合并镜像 | `<registry>/spacegate-admin:<version>` |
| AI Gateway Service 镜像 | `<registry>/ai-gateway-service:<version>` |
| Wasm 文件 | SpaceGate 镜像内 `/lib/spacegate/wasm/spacegate_plugin_ai_gateway_queue.wasm` |
| 可选 Wasm OCI Artifact | `oci://<registry>/ai-gateway-queue:<version>` |

版本建议使用不可变 tag，例如：

```text
v2026.07.08-<git-short-sha>
```

不要在生产 manifest 中使用 `latest`、`dev`、`test` 或可变 tag。

### 4.3 外部依赖

生产必须确认：

- Redis 使用托管 Redis、Sentinel 或 Cluster，不使用示例单副本 Redis。
- 如请求体可能超过 `AI_INLINE_THRESHOLD`，配置 S3/MinIO 兼容对象存储。
- 回调模式启用 HTTPS 校验：`AI_REQUIRE_HTTPS_CALLBACK=true`。
- OTEL 使用生产 Collector；如果使用 ClickHouse，需要持久化、认证、备份、TTL 和容量规划。
- 私有 OCI Wasm 仓库需要配置 `imagePullSecret` 或等效认证 Secret。

## 5. 构建和发布制品

以下命令假设在工作区根目录执行：

```bash
cd /Users/yiye/projectSpace/[REDACTED]_project/spacegate-workspace
export WORKSPACE_ROOT="$PWD"
export SPACEGATE_ROOT="$WORKSPACE_ROOT/spacegate"
export ADMIN_FE_ROOT="$WORKSPACE_ROOT/spacegate-admin-fe"
export REGISTRY="registry.example.com/spacegate"
export VERSION="v2026.07.10-002"
export SPACEGATE_IMAGE="${REGISTRY}/spacegate:${VERSION}"
export SPACEGATE_ADMIN_IMAGE="${REGISTRY}/spacegate-admin:${VERSION}"
export AI_GATEWAY_SERVICE_IMAGE="${REGISTRY}/ai-gateway-service:${VERSION}"
export ADMIN_NGINX_IMAGE="nginx:1.27-bookworm"
```

如果执行 `docker build -t "$SPACEGATE_ADMIN_IMAGE"` 时报错 `invalid tag ""`，说明当前 shell 中 `SPACEGATE_ADMIN_IMAGE` 没有设置或为空。先重新执行上面的变量块，再执行镜像构建命令。

如果执行 Admin 镜像构建时报错 `path "/resource/docker/spacegate-admin" not found`，说明当前 shell 中 `SPACEGATE_ROOT` 没有设置或为空。先重新执行上面的变量块，并确认：

```bash
test -d "$SPACEGATE_ROOT/resource/docker/spacegate-admin"
test -d "$ADMIN_FE_ROOT"
```

### 5.1 构建 SpaceGate K8S 镜像

如果生产镜像需要包含 `hai-hub-spacegate-plugins` dylib，使用当前仓库已有 Dockerfile：

```bash
docker build \
  --build-context hai_hub=/path/to/hai-hub \
  -f spacegate/resource/docker/spacegate-k8s/Dockerfile \
  -t "$SPACEGATE_IMAGE" \
  spacegate

docker push "$SPACEGATE_IMAGE"
```

说明：

- 该 Dockerfile 会构建 `spacegate`，启用 `build-k8s,wasm,dylib,static-openssl`。
- 该 Dockerfile 会从 `hai_hub` build context 构建 `libhai_hub_spacegate_plugins.so` 并复制到 `/lib/spacegate/plugins/`。
- 生产 DaemonSet 可通过 `PLUGINS=/lib/spacegate/plugins,/var/lib/spacegate/plugins` 同时扫描镜像内置插件和 K8s 挂载插件；不要把 volume 直接挂载覆盖 `/lib/spacegate/plugins`。
- 动态库只在 SpaceGate 进程启动时扫描加载；插件代码更新后必须重新构建镜像并滚动重启 DaemonSet。
- 如果生产不需要 HAI dylib，请使用不含 `hai-plugin-builder` 阶段的生产 Dockerfile，并保持启动参数 `-c k8s:spacegate`。

### 5.2 构建 AI Gateway Service 镜像

```bash
docker build \
  -f spacegate/resource/docker/ai-gateway-service/Dockerfile \
  -t "$AI_GATEWAY_SERVICE_IMAGE" \
  spacegate

docker push "$AI_GATEWAY_SERVICE_IMAGE"
```

### 5.3 构建 SpaceGate Admin 合并镜像

当前推荐构建一个 `spacegate-admin` 合并镜像，镜像内包含：

- Nginx：监听 `9080`，提供 `spacegate-admin-fe/dist` 静态资源。
- Admin Server：容器内监听 `9081`。
- Nginx `/api` 反向代理到容器内 `127.0.0.1:9081`。

先构建 Admin SDK 和前端静态资源：

```bash
cd "$SPACEGATE_ROOT/sdk/admin-client"
npm ci
npm run build

cd "$ADMIN_FE_ROOT"
npm ci
npm run build
```

再准备 Docker build context 并构建合并镜像：

```bash
cd "$WORKSPACE_ROOT"

export ADMIN_DOCKER_CONTEXT=""
export ADMIN_DOCKER_DIR="/resource/docker/spacegate-admin"

test -d "$ADMIN_DOCKER_CONTEXT"
test -f "$ADMIN_DOCKER_DIR/Dockerfile"
test -f "$ADMIN_FE_ROOT/dist/index.html"

rsync -a --delete "$ADMIN_FE_ROOT/dist/" "$ADMIN_DOCKER_DIR/dist/"
test -f "$ADMIN_DOCKER_DIR/dist/index.html"

docker build --progress=plain \
  --build-context "spacegate_src=$SPACEGATE_ROOT" \
  --build-arg "NGINX_IMAGE=$ADMIN_NGINX_IMAGE" \
  -f "$ADMIN_DOCKER_DIR/Dockerfile" \
  -t "$SPACEGATE_ADMIN_IMAGE" \
  "$ADMIN_DOCKER_CONTEXT"

docker push "$SPACEGATE_ADMIN_IMAGE"
```

当前合并镜像的前端运行时路径约定：

| 路径 | 反向代理目标 |
| --- | --- |
| `/` | `spacegate-admin-fe/dist` 静态资源 |
| `/api/` | 容器内 `127.0.0.1:9081` |
| `/ai-gateway/` | ConfigMap 中固定的 `http://ai-gateway-queue:18080` |

### 5.4 Wasm 插件分发方式

生产把 `ai-gateway-queue` 作为系统 Wasm 内置到 SpaceGate 镜像；`/plugins` hostPath 只保留给外置扩展：

```text
container: /lib/spacegate/wasm/spacegate_plugin_ai_gateway_queue.wasm
SgFilter:  file:///lib/spacegate/wasm/spacegate_plugin_ai_gateway_queue.wasm
```

构建镜像时校验 Wasm 制品并以不可变镜像 tag 发布，再 rollout DaemonSet。完整操作、校验和回滚步骤以 `deploy/k8s/ai-gateway/production/README.md` 为准。

## 6. 部署 SpaceGate 基础组件

### 6.1 安装 Gateway API

生产建议把 Gateway API manifest 固定到内部制品库。直接使用公网命令时需要确认网络和版本：

```bash
kubectl apply -f https://github.com/kubernetes-sigs/gateway-api/releases/download/v0.6.2/standard-install.yaml
```

或使用仓库内镜像源替换版：

```bash
kubectl apply -f spacegate/resource/kube-manifests/gateway-api-0.6.2-experimental-china.yaml
```

### 6.2 安装 SpaceGate namespace、GatewayClass、CRD、DaemonSet

先将 `spacegate/resource/kube-manifests/spacegate-gateway.yaml` 中的 `registry.example.com/spacegate/spacegate:REPLACE_ME` 替换为 `${SPACEGATE_IMAGE}`。不要先 apply 再通过 `kubectl set image` 修正，因为基础清单不会使用 `latest` 或其他可变镜像作为过渡版本。

```bash
kubectl apply -f spacegate/resource/kube-manifests/namespace.yaml
kubectl apply -f spacegate/resource/kube-manifests/gatewayclass.yaml
kubectl apply -f spacegate/resource/kube-manifests/spacegate-httproute.yaml
kubectl apply -f spacegate/resource/kube-manifests/spacegate-mcproute.yaml
kubectl apply -f spacegate/resource/kube-manifests/higress-wasmplugin-crd.yaml
kubectl apply -f spacegate/resource/kube-manifests/spacegate-gateway.yaml
```

```bash
kubectl rollout status daemonset/spacegate -n spacegate --timeout=300s
```

### 6.3 配置 SpaceGate OTEL

如果生产使用统一 Collector：

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

注意：

- `SPACEGATE_OTEL_*` 是进程级配置，修改后需要 DaemonSet rollout。
- `deploy/k8s/test-spacegate/otel-stack.yaml` 只适合测试；生产 ClickHouse 必须有持久化、认证、资源限制和 TTL。
- 审计中心主要查 `otel_logs` 中 `LogAttributes['event']='http_access'` 的记录；插件业务字段在 `LogAttributes['telemetry']` JSON 中。

## 7. 部署 AI Gateway 后端

AI Gateway 的唯一生产入口是 `deploy/k8s/ai-gateway/production/README.md`。该手册创建 `ai-gateway-queue-redis` Secret 和 `ai-gateway-queue-runtime` ConfigMap，并部署 Kubernetes Service `ai-gateway-queue:18080`。不要使用旧示例中的 `ai-gateway-runtime`、`ai-gateway-service` 或单副本 Redis 清单。

## 8. 部署 SpaceGate Admin

当前推荐使用合并镜像部署一个 `spacegate-admin` Deployment。当前仓库的基础 manifest 已包含：

- `ServiceAccount/ClusterRole/ClusterRoleBinding`
- `Deployment/spacegate-admin`
- `Service/spacegate-admin`，端口 `9080`

部署并替换为生产镜像：

```bash
kubectl apply -f spacegate/resource/kube-manifests/spacegate-admin-server.yaml

kubectl set image deployment/spacegate-admin \
  spacegate-admin="$SPACEGATE_ADMIN_IMAGE" \
  -n spacegate

kubectl rollout status deployment/spacegate-admin -n spacegate --timeout=300s

kubectl get svc spacegate-admin -n spacegate
```

生产建议检查 `ClusterRole` 权限，只保留管理界面需要的资源。当前 manifest 为了管理 Gateway、HTTPRoute、SgFilter、WasmPlugin，权限范围较宽。

前端对外暴露建议使用 Ingress 或统一网关，而不是长期依赖 `LoadBalancer` 示例默认值。Nginx 或 Ingress 必须保证：

```text
/api/        -> http://spacegate-admin.spacegate.svc.cluster.local:9080/api/
/ai-gateway/ -> http://ai-gateway-queue.spacegate.svc.cluster.local:18080/
/           -> 前端静态资源
```

当前 `spacegate-admin` 的 Nginx 由 `spacegate-admin-nginx` ConfigMap 挂载；更新该 ConfigMap 后必须 rollout `spacegate-admin` Deployment。`/ai-gateway` 固定代理到 `ai-gateway-queue:18080`。

## 9. 配置业务网关、路由和插件

生产 Gateway、HTTPRoute 和 SgFilter 仅维护在 `deploy/k8s/ai-gateway/production/`：

- 默认 `kustomization.yaml` 复用既有 Gateway，运维需将 `httproute-ai.yaml.spec.parentRefs.name` 改成实际 Gateway 名称。
- 仅在确认 `9993` 未被占用时，使用 `deploy/k8s/ai-gateway/production-with-dedicated-gateway/` overlay 创建 `Gateway/ai-gateway`。
- `SgFilter/ai-gateway-queue` 只挂载到 `HTTPRoute/ai-api` 一次，避免双倍限流。
- Wasm 固定读取 `file:///lib/spacegate/wasm/spacegate_plugin_ai_gateway_queue.wasm`，服务 cluster 固定为 `ai-gateway-queue:18080`。
- `MCPRoute` / `HTTPSpaceroute` 的 status 子资源当前未实现控制器状态回写。请使用 Gateway、后端连通性和 SpaceGate 日志判断生效状态，不要仅依赖 Route status。

## 10. OTEL 和审计入库

生产推荐使用统一 Collector：

```text
SpaceGate / services
  -> OTLP gRPC 4317
  -> OpenTelemetry Collector
  -> ClickHouse / observability backend
```

如果沿用 ClickHouse：

- 不要使用 `clickhouse/clickhouse-server:latest`。
- 不要使用空密码。
- 不要使用 `emptyDir`。
- 配置 PVC、资源限制、备份、TTL、冷热分层或归档策略。
- `create_schema: true` 可以由 Collector ClickHouse exporter 创建 OTel 原始表，但业务高频查询字段应通过物化视图或派生表治理。

核心表：

| 信号 | 表 |
| --- | --- |
| Logs | `otel_logs` |
| Traces | `otel_traces` |
| Metrics | `otel_metrics_sum`、`otel_metrics_histogram`、`otel_metrics_gauge` 等 |

审计中心以 logs 为主：

```sql
SELECT
  Timestamp,
  LogAttributes['request_id'] AS request_id,
  LogAttributes['path'] AS path,
  LogAttributes['status_code'] AS status_code,
  LogAttributes['telemetry'] AS telemetry
FROM otel_logs
WHERE LogAttributes['event'] = 'http_access'
ORDER BY Timestamp DESC
LIMIT 20;
```

## 11. 上线验证

### 11.1 基础资源

```bash
kubectl get namespace spacegate
kubectl get gatewayclass
kubectl get daemonset spacegate -n spacegate
kubectl get pods -n spacegate -o wide
```

### 11.2 工作负载

```bash
kubectl rollout status daemonset/spacegate -n spacegate --timeout=300s
kubectl rollout status deployment/spacegate-admin -n spacegate --timeout=300s
kubectl rollout status deployment/ai-gateway-queue -n spacegate --timeout=300s
```

### 11.3 配置资源

```bash
kubectl get gateway -n spacegate
kubectl get httproute -n spacegate
kubectl get sgfilter -n spacegate
kubectl get wasmplugin -n spacegate
```

### 11.4 后端健康检查

```bash
kubectl run curl-ai-gateway-health --rm -i --restart=Never -n spacegate \
  --image=curlimages/curl:8.5.0 --quiet -- \
  curl -sf http://ai-gateway-queue:18080/healthz
```

### 11.5 管理前端检查

```bash
kubectl port-forward -n spacegate svc/spacegate-admin 9080:9080
```

浏览器访问：

```text
http://127.0.0.1:9080/
```

确认：

- 页面静态资源加载成功。
- `/api` 能访问容器内 Admin Server。
- `/ai-gateway` 能访问 `ai-gateway-queue` 管理接口。

SpaceGate DaemonSet 完成首次部署或滚动升级后，手动刷新 Admin 进程内的原生插件属性缓存：

```bash
curl -fsS -X POST http://127.0.0.1:9080/api/plugin/refresh
```

接口成功时直接返回最新的插件属性数组，可检查结果中是否包含 `hai-auth`、`hai-asset`、`hai-quota`、`hai-dispatch` 和 `hai-observe`。刷新失败不会覆盖已有缓存，也不会启动一小时的缓存等待时间；修复网关发现或连通性后可以立即重试。

如果 Admin 配置了 `KEY` 和 `SK`，先登录并复用服务端返回的 JWT Cookie：

```bash
curl -fsS -c /tmp/spacegate-admin.cookie \
  -H 'Content-Type: application/json' \
  -d "{\"ak\":\"admin\",\"sk\":\"${ADMIN_LOGIN_SK}\"}" \
  http://127.0.0.1:9080/api/auth/login

curl -fsS -b /tmp/spacegate-admin.cookie \
  -X POST http://127.0.0.1:9080/api/plugin/refresh
```

### 11.6 数据面请求检查

先配置测试租户限流：

```bash
kubectl port-forward -n spacegate svc/ai-gateway-queue 18080:18080

curl -sf -X PUT http://127.0.0.1:18080/v1/admin/tenant-rate-limits \
  -H 'Content-Type: application/json' \
  -d '{"tenant":"prod-smoke","rps":5,"burst":5,"cost":1}'
```

再从业务入口验证：

```bash
curl -i http://<node-ip-or-lb>:9993/v1/chat/completions \
  -H 'X-RateLimit-Policy: abandon' \
  -H 'X-Tenant-Id: prod-smoke' \
  -H 'Content-Type: application/json' \
  -d '{"prompt":"health check"}'
```

### 11.7 OTEL 检查

```bash
kubectl logs -n spacegate -l app=spacegate --tail=100
kubectl logs -n <observability-namespace> deployment/<otel-collector-deployment> --tail=100
```

如果使用 ClickHouse：

```bash
clickhouse-client --database otel --query "
  SELECT 'otel_logs' AS table, count() FROM otel_logs
  UNION ALL SELECT 'otel_traces', count() FROM otel_traces
  UNION ALL SELECT 'otel_metrics_sum', count() FROM otel_metrics_sum
"
```

## 12. 升级流程

### 12.1 常规升级顺序

1. 构建并推送新版本镜像和 Wasm OCI 制品。
2. 更新 `ai-gateway-queue` Deployment 使用的镜像。
3. 更新 `spacegate-admin` 镜像。
4. 更新 SpaceGate DaemonSet 镜像。
5. 更新 SgFilter 中的 Wasm `url` 和 `sha256`。
6. 等待 rollout。
7. 调用 `POST /api/plugin/refresh` 更新 Admin 的原生插件属性缓存。
8. 执行第 11 节验证。

### 12.2 更新镜像

```bash
kubectl set image deployment/ai-gateway-queue \
  ai-gateway-service="$AI_GATEWAY_SERVICE_IMAGE" \
  -n spacegate

kubectl set image deployment/spacegate-admin \
  spacegate-admin="$SPACEGATE_ADMIN_IMAGE" \
  -n spacegate

kubectl set image daemonset/spacegate \
  spacegate="$SPACEGATE_IMAGE" \
  -n spacegate
```

### 12.3 更新 Wasm 插件

如果使用镜像内置 Wasm，更新流程是重新构建并发布 SpaceGate 镜像，然后滚动更新 SpaceGate DaemonSet。SgFilter 的 `url` 保持不变：

```yaml
url: file:///lib/spacegate/wasm/spacegate_plugin_ai_gateway_queue.wasm
```

如果使用可选 OCI Wasm，修改生产 SgFilter：


```yaml
url: oci://<registry>/ai-gateway-queue:<new-version>
sha256: sha256:<new-digest>
```

应用：

```bash
kubectl apply -f <production-sgfilter.yaml>
```

如果 SpaceGate 已缓存旧模块，建议同时更新 tag、`sha256`，必要时调整 `plugin_vm_id` 或相关 cache key 以触发重新拉取，具体以当前插件加载实现和生产配置为准。

## 13. 回滚流程

### 13.1 回滚 Deployment

```bash
kubectl rollout undo deployment/ai-gateway-queue -n spacegate
kubectl rollout undo deployment/spacegate-admin -n spacegate
```

### 13.2 回滚 SpaceGate DaemonSet

```bash
kubectl rollout history daemonset/spacegate -n spacegate
kubectl rollout undo daemonset/spacegate -n spacegate
kubectl rollout status daemonset/spacegate -n spacegate --timeout=300s
```

### 13.3 回滚 Wasm

把 SgFilter 中的 `url` 和 `sha256` 改回上一版：

```bash
kubectl apply -f <previous-production-sgfilter.yaml>
```

回滚后必须重新执行：

```bash
kubectl get pods -n spacegate
kubectl get sgfilter ai-gateway-queue -n spacegate -o yaml
curl -i http://<node-ip-or-lb>:9993/v1/chat/completions \
  -H 'X-RateLimit-Policy: abandon' \
  -H 'X-Tenant-Id: prod-smoke' \
  -H 'Content-Type: application/json' \
  -d '{"prompt":"rollback check"}'
```

## 14. 生产检查清单

上线前逐项确认：

- [ ] 所有镜像使用不可变生产 tag。
- [ ] 没有 `latest`、`dev`、`test`、`Never`、本地 registry。
- [ ] 没有部署 `mock-upstream`。
- [ ] Wasm 使用 OCI URL，并记录 `sha256`。
- [ ] 插件只挂载一次，没有 Gateway 和 HTTPRoute 重复挂载。
- [ ] Redis 是生产级实例，有持久化、高可用和监控。
- [ ] `AI_REQUIRE_HTTPS_CALLBACK=true`。
- [ ] 大 body 场景已配置对象存储，或明确限制 `AI_MAX_BODY_BYTES`。
- [ ] `spacegate-admin` RBAC 已经过权限评审。
- [ ] 管理前端 `/api` 和 `/ai-gateway` 反代路径可用。
- [ ] SpaceGate `SPACEGATE_OTEL_*` 已指向生产 Collector。
- [ ] ClickHouse 或其他 observability 后端有持久化、认证、备份和 TTL。
- [ ] `kubectl rollout status` 全部成功。
- [ ] 数据面 smoke test 成功。
- [ ] 审计日志 `http_access` 可查询。
- [ ] 回滚版本和回滚命令已准备。

## 15. 文件索引

| 文件 | 说明 |
| --- | --- |
| `Dockerfile.all-in-one` | 单容器开发/演示方案，生产禁用 |
| `docker/all-in-one/start.sh` | all-in-one 进程启动脚本，生产禁用 |
| `spacegate/docs/k8s/installation.md` | SpaceGate K8S 基础安装说明 |
| `spacegate/docs/k8s/test-deployment.md` | 测试环境部署教程 |
| `spacegate/deploy/README.md` | AI Gateway 编译、部署、OCI Wasm 说明 |
| `spacegate/deploy/push-wasm-oci.sh` | Wasm OCI 发布脚本 |
| `spacegate/resource/docker/spacegate-k8s/Dockerfile` | SpaceGate K8S 镜像构建参考 |
| `spacegate/deploy/k8s/test-spacegate/otel-stack.yaml` | 测试 OTEL + ClickHouse 栈 |
| `spacegate/deploy/k8s/ai-gateway/ai-gateway-service.yaml` | AI Gateway Service K8S 示例 |
| `spacegate/resource/kube-manifests/spacegate-admin-server.yaml` | SpaceGate Admin 合并镜像 K8S 示例 |
| `spacegate/deploy/k8s/ai-gateway/sgfilter-ai-gateway-queue.yaml` | SgFilter 插件挂载示例 |
| `spacegate/binary/ai-gateway-service/README.md` | AI Gateway Service 参数和接口说明 |

## 16. 制品构建清单

本节只说明从 `spacegate-workspace` 执行什么命令、生成什么制品、制品放到哪里。以下命令必须在同一个 shell 中按顺序执行，先执行变量块。

### 16.1 公共变量

执行目录：`spacegate-workspace`

```bash
cd /Users/yiye/projectSpace/[REDACTED]_project/spacegate-workspace

export WORKSPACE_ROOT="$PWD"
export SPACEGATE_ROOT="$WORKSPACE_ROOT/spacegate"
export ADMIN_FE_ROOT="$WORKSPACE_ROOT/spacegate-admin-fe"
export HAI_HUB_ROOT="/Users/yiye/projectSpace/[REDACTED]_project/hai-hub"
export ARTIFACT_DIR="$WORKSPACE_ROOT/image-artifacts"
export SPACEGATE_ADMIN_CONFIG_DIR="$WORKSPACE_ROOT/spacegate-admin-config"

export VERSION="v2026.07.10-001"
export REGISTRY="registry.example.com/spacegate"

export SPACEGATE_IMAGE="${REGISTRY}/spacegate:${VERSION}"
export SPACEGATE_ADMIN_IMAGE="${REGISTRY}/spacegate-admin:${VERSION}"
export AI_GATEWAY_SERVICE_IMAGE="${REGISTRY}/ai-gateway-service:${VERSION}"
export WASM_OCI_REF="${REGISTRY}/ai-gateway-queue:${VERSION}"

export ADMIN_NGINX_IMAGE="nginx:1.27-bookworm"

# 示例值仅用于本地验证，生产必须替换。
export ADMIN_JWT_KEY="c3BhY2VnYXRlLWFkbWluLWp3dC1zZWNyZXQ="
export ADMIN_LOGIN_SK="admin-123456-change-me"

mkdir -p "$ARTIFACT_DIR" "$ARTIFACT_DIR/native-plugins" "$ARTIFACT_DIR/wasm" "$SPACEGATE_ADMIN_CONFIG_DIR"

test -d "$SPACEGATE_ROOT"
test -d "$ADMIN_FE_ROOT"
test -d "$HAI_HUB_ROOT"
```

制品目录：

```text
spacegate-workspace/image-artifacts/
```

### 16.2 SpaceGate 镜像

该镜像同时内置：

- SpaceGate 二进制：`/usr/local/bin/spacegate`
- native 插件：`/lib/spacegate/plugins/hai_hub_spacegate_plugins.so`
- `ai-gateway-queue` Wasm 插件：`/lib/spacegate/wasm/spacegate_plugin_ai_gateway_queue.wasm`

执行命令：

```bash
cd "$WORKSPACE_ROOT"

docker build --progress=plain \
  --build-context "hai_hub=$HAI_HUB_ROOT" \
  -f "$SPACEGATE_ROOT/resource/docker/spacegate-k8s/Dockerfile" \
  -t "$SPACEGATE_IMAGE" \
  "$SPACEGATE_ROOT"

docker save "$SPACEGATE_IMAGE" \
  -o "$ARTIFACT_DIR/spacegate-${VERSION}.tar"
```

生成制品：

| 制品 | 放置目录 |
| --- | --- |
| `spacegate-${VERSION}.tar` | `spacegate-workspace/image-artifacts/` |

### 16.3 内置插件归档

`hai-hub-spacegate-plugin` native 插件和 `ai-gateway-queue` Wasm 插件都由 SpaceGate 镜像构建过程生成并内置到镜像中。单独归档时，从已经构建好的 SpaceGate 镜像中复制出来，便于 U 盘离线交付和校验。

执行命令：

```bash
cd "$WORKSPACE_ROOT"

plugin_container_id="$(docker create "$SPACEGATE_IMAGE")"
docker cp "$plugin_container_id:/lib/spacegate/plugins/hai_hub_spacegate_plugins.so" \
  "$ARTIFACT_DIR/native-plugins/hai_hub_spacegate_plugins.so"
docker cp "$plugin_container_id:/lib/spacegate/wasm/spacegate_plugin_ai_gateway_queue.wasm" \
  "$ARTIFACT_DIR/wasm/spacegate_plugin_ai_gateway_queue.wasm"
docker rm -f "$plugin_container_id"

test -f "$ARTIFACT_DIR/native-plugins/hai_hub_spacegate_plugins.so"
test -f "$ARTIFACT_DIR/wasm/spacegate_plugin_ai_gateway_queue.wasm"
```

生成制品：

| 制品 | 放置目录 |
| --- | --- |
| `hai_hub_spacegate_plugins.so` | `spacegate-workspace/image-artifacts/native-plugins/` |
| `spacegate_plugin_ai_gateway_queue.wasm` | `spacegate-workspace/image-artifacts/wasm/` |

### 16.4 SpaceGate Admin 合并镜像

执行命令：

```bash
cd "$SPACEGATE_ROOT/sdk/admin-client"
npm ci
npm run build

cd "$ADMIN_FE_ROOT"
npm ci
npm run build

cd "$WORKSPACE_ROOT"
export ADMIN_DOCKER_CONTEXT=""
export ADMIN_DOCKER_DIR="/resource/docker/spacegate-admin"

rsync -a --delete "$ADMIN_FE_ROOT/dist/" "$ADMIN_DOCKER_DIR/dist/"
test -f "$ADMIN_DOCKER_DIR/dist/index.html"

docker build --progress=plain \
  --build-context "spacegate_src=$SPACEGATE_ROOT" \
  --build-arg "NGINX_IMAGE=$ADMIN_NGINX_IMAGE" \
  -f "$ADMIN_DOCKER_DIR/Dockerfile" \
  -t "$SPACEGATE_ADMIN_IMAGE" \
  "$ADMIN_DOCKER_CONTEXT"

docker save "$SPACEGATE_ADMIN_IMAGE" \
  -o "$ARTIFACT_DIR/spacegate-admin-${VERSION}.tar"
```

生成制品：

| 制品 | 放置目录 |
| --- | --- |
| `spacegate-admin-${VERSION}.tar` | `spacegate-workspace/image-artifacts/` |

启动方式：

```text
CMD ["sh", "-c", "sh start.sh"]
```

容器内实际启动进程：

```bash
./admin-server -H 127.0.0.1 -p 9081 -c "$CONFIG" &
nginx -g 'daemon off;'
```

端口：

| 端口 | 是否需要暴露 | 说明 |
| --- | --- | --- |
| `9080` | 是 | Nginx 对外端口，提供 Admin 前端静态资源，并把 `/api` 反代到容器内 Admin Server |
| `9081` | 否 | 容器内 Admin Server 端口，只被本容器 Nginx 访问 |

环境变量：

| 变量 | 是否必需 | 示例 | 说明 |
| --- | --- | --- | --- |
| `CONFIG` | 是 | `k8s:spacegate` | Admin Server 配置后端。K8s 部署使用 `k8s:<namespace>`；文件模式可用 `file:/etc/spacegate` |
| `GATEWAY_CLASS_NAME` | K8s 模式建议显式设置 | `spacegate` | Admin 创建、查询、列出和监听的 `GatewayClass`；Spacegate 数据面也必须使用相同值；未设置时兼容默认值 `spacegate` |
| `GATEWAY_INSTANCE` | 否 | `spacegate.spacegate` | 插件发现访问的 Spacegate DaemonSet，格式为 `<daemonset>[.<namespace>]`；未设置时先读取所选 `GatewayClass` 的 `app.kubernetes.io/instance` 标签，再回退到 `spacegate.spacegate` |
| `RUST_LOG` | 否 | `info` | Rust 日志级别 |
| `AI_GATEWAY_SERVICE_URL` | 否 | `http://ai-gateway-queue:18080` | 仅未挂载 Nginx ConfigMap 的本地 Docker 镜像使用；K8s 由 `spacegate-admin-nginx` ConfigMap 固定代理目标 |
| `KEY` | 否 | `c3BhY2VnYXRlLWFkbWluLWp3dC1zZWNyZXQ=` | JWT 签名密钥，必须是 base64 字符串；设置后接口会校验 `Authorization: Bearer <jwt>` 或 `jwt` Cookie |
| `SK` | 否 | `admin-123456-change-me` | 登录口令，设置后 `/auth/login` 会校验请求体中的 `sk` |

本地 `docker run` 示例：

```bash
rsync -a --delete "$SPACEGATE_ROOT/resource/install/default-config/" "$SPACEGATE_ADMIN_CONFIG_DIR/"
test -f "$SPACEGATE_ADMIN_CONFIG_DIR/config.json"

docker run --rm \
  --name spacegate-admin \
  -p 9080:9080 \
  -v "$SPACEGATE_ADMIN_CONFIG_DIR:/etc/spacegate" \
  -e CONFIG=file:/etc/spacegate \
  -e RUST_LOG=info \
  -e AI_GATEWAY_SERVICE_URL=http://host.docker.internal:18080 \
  "$SPACEGATE_ADMIN_IMAGE"
```

K8s 配置后端 `docker run` 示例：

```bash
docker run --rm \
  --name spacegate-admin \
  -p 9080:9080 \
  -v "$HOME/.kube:/root/.kube:ro" \
  -e CONFIG=k8s:spacegate \
  -e GATEWAY_CLASS_NAME=spacegate \
  -e GATEWAY_INSTANCE=spacegate.spacegate \
  -e RUST_LOG=info \
  -e AI_GATEWAY_SERVICE_URL=http://ai-gateway-queue:18080 \
  "$SPACEGATE_ADMIN_IMAGE"
```

如果启用登录校验：

```bash
docker run --rm \
  --name spacegate-admin \
  -p 9080:9080 \
  -v "$HOME/.kube:/root/.kube:ro" \
  -e CONFIG=k8s:spacegate \
  -e GATEWAY_CLASS_NAME=spacegate \
  -e GATEWAY_INSTANCE=spacegate.spacegate \
  -e RUST_LOG=info \
  -e AI_GATEWAY_SERVICE_URL=http://ai-gateway-queue:18080 \
  -e KEY="$ADMIN_JWT_KEY" \
  -e SK="$ADMIN_LOGIN_SK" \
  "$SPACEGATE_ADMIN_IMAGE"
```

启动后访问：

```text
http://127.0.0.1:9080/
```

说明：

- 本地 `file:/etc/spacegate` 模式需要挂载宿主机目录到容器内 `/etc/spacegate`，并且目录里必须存在 `config.json`；上面的 `rsync ... default-config/` 和 `-v "$SPACEGATE_ADMIN_CONFIG_DIR:/etc/spacegate"` 就是该初始化和挂载。
- 如果没有初始化 `config.json`，访问 `/api/discovery/instance/health` 会返回 `500`，后端日志通常是 `No such file or directory (os error 2)`。
- 未挂载 ConfigMap 时，`/ai-gateway` 会反代到 `AI_GATEWAY_SERVICE_URL`。本地 Docker 访问宿主机服务时通常使用 `http://host.docker.internal:18080`；K8s 使用 `spacegate-admin-nginx` ConfigMap 中的 `http://ai-gateway-queue:18080`。
- `k8s:spacegate` 模式需要容器运行环境能访问 Kubernetes API，并具备对应 RBAC；本地 `docker run` 需要挂载 kubeconfig，例如 `-v "$HOME/.kube:/root/.kube:ro"`。
- 如果挂载 kubeconfig 后仍无法连接 Kubernetes API，检查 kubeconfig 里的 `clusters[].cluster.server`。如果它是 `https://127.0.0.1:<port>`，容器内的 `127.0.0.1` 指向容器自身，通常需要改成 Docker 可访问的宿主机地址，例如 `https://host.docker.internal:<port>`，或直接在 K8s Pod 内运行该镜像。
- K8s 模式不需要 `/etc/spacegate/config.json` 这类 file backend 初始化数据；它读取 Kubernetes 资源。部署前必须先安装 Gateway API/SpaceGate CRD、`GatewayClass(spacegate)`、SpaceGate DaemonSet、ServiceAccount 和 RBAC。否则 `/api/discovery/instance/health` 会因为找不到 `GatewayClass` 或 SpaceGate 实例返回错误。
- `GATEWAY_CLASS_NAME` 是 Admin 和 Spacegate 进程启动时读取的运行时配置；`GATEWAY_INSTANCE` 只由 Admin 的实例发现使用。修改 Deployment/DaemonSet 中的值后必须滚动重启对应工作负载，正在运行的进程不会热加载环境变量。
- `GATEWAY_INSTANCE` 指向 Spacegate DaemonSet，不是业务 `Gateway`。例如 namespace 为 `ai-hai`、GatewayClass 和 DaemonSet 都叫 `ai-spacegate`、业务 Gateway 叫 `ai-gateway` 时，应配置 `CONFIG=k8s:ai-hai`、`GATEWAY_CLASS_NAME=ai-spacegate`、`GATEWAY_INSTANCE=ai-spacegate.ai-hai`；不要把 `GATEWAY_INSTANCE` 写成 `ai-gateway.ai-hai`。
- 同一 Admin 只管理 `GATEWAY_CLASS_NAME` 指定的 GatewayClass：新建 Gateway 会写入该值，名称列表、详情读取和事件监听也使用相同过滤条件，不会再混入同 namespace 下其他 GatewayClass 的 Gateway。
- Spacegate DaemonSet 必须配置与 Admin 相同的 `CONFIG` 和 `GATEWAY_CLASS_NAME`。否则 Admin 虽然能创建和展示 Gateway，数据面进程不会监听该 GatewayClass，配置不会实际生效。
- 该运行时配置能力同时修改了 Admin Server 和 Spacegate 二进制；升级时必须重新构建并发布两个镜像，再分别滚动更新 Deployment 和 DaemonSet。只更新 YAML 环境变量但继续使用旧镜像不会生效。

K8s 容器配置示例：

```yaml
containers:
  - name: spacegate-admin
    image: registry.example.com/spacegate/spacegate-admin:v2026.07.10-001
    ports:
      - containerPort: 9080
    env:
      - name: CONFIG
        value: k8s:spacegate
      - name: GATEWAY_CLASS_NAME
        value: spacegate
      - name: GATEWAY_INSTANCE
        value: spacegate.spacegate
      - name: RUST_LOG
        value: info
      - name: AI_GATEWAY_SERVICE_URL
        value: http://ai-gateway-queue:18080
```

### 16.5 AI Gateway Service 镜像

执行命令：

```bash
cd "$WORKSPACE_ROOT"

docker build --progress=plain \
  -f "$SPACEGATE_ROOT/resource/docker/ai-gateway-service/Dockerfile" \
  -t "$AI_GATEWAY_SERVICE_IMAGE" \
  "$SPACEGATE_ROOT"

docker save "$AI_GATEWAY_SERVICE_IMAGE" \
  -o "$ARTIFACT_DIR/ai-gateway-service-${VERSION}.tar"
```

生成制品：

| 制品 | 放置目录 |
| --- | --- |
| `ai-gateway-service-${VERSION}.tar` | `spacegate-workspace/image-artifacts/` |

### 16.6 可选 Wasm OCI Artifact

默认部署不需要推送 OCI Artifact。构建出的 Wasm 文件随 SpaceGate 镜像交付；SgFilter 使用以下路径：

```text
file:///lib/spacegate/wasm/spacegate_plugin_ai_gateway_queue.wasm
```

只有在需要独立分发或独立升级 Wasm 插件时，才额外推送 OCI Artifact。

执行命令：

```bash
cd "$SPACEGATE_ROOT"

rustup target add wasm32-wasip1

cargo build --release \
  --target wasm32-wasip1 \
  --manifest-path plugins/wasm/Cargo.toml \
  -p spacegate_plugin_ai_gateway_queue

export WASM_FILE="$SPACEGATE_ROOT/plugins/wasm/target/wasm32-wasip1/release/spacegate_plugin_ai_gateway_queue.wasm"
test -f "$WASM_FILE"

cp "$WASM_FILE" "$ARTIFACT_DIR/wasm/spacegate_plugin_ai_gateway_queue-${VERSION}.wasm"

export WASM_SHA256="$(shasum -a 256 "$WASM_FILE" | awk '{print $1}')"

oras push "$WASM_OCI_REF" \
  --artifact-type application/vnd.module.wasm.content.layer.v1+wasm \
  "$WASM_FILE:application/wasm"

cat > "$ARTIFACT_DIR/WASM-OCI.txt" <<EOF
WASM_OCI_URL=oci://${WASM_OCI_REF}
WASM_SHA256=sha256:${WASM_SHA256}
LOCAL_WASM=wasm/spacegate_plugin_ai_gateway_queue-${VERSION}.wasm
EOF
```

生成制品：

| 制品 | 放置目录 |
| --- | --- |
| `spacegate_plugin_ai_gateway_queue-${VERSION}.wasm` | `spacegate-workspace/image-artifacts/wasm/` |
| `WASM-OCI.txt` | `spacegate-workspace/image-artifacts/` |
| OCI Artifact | `${WASM_OCI_REF}` 指向的 OCI registry |

### 16.7 校验和最终交付目录

执行命令：

```bash
cd "$WORKSPACE_ROOT"

shasum -a 256 \
  "$ARTIFACT_DIR"/*.tar \
  "$ARTIFACT_DIR/native-plugins"/*.so \
  "$ARTIFACT_DIR/wasm"/*.wasm \
  > "$ARTIFACT_DIR/SHA256SUMS"

cat > "$ARTIFACT_DIR/IMAGES.txt" <<EOF
VERSION=${VERSION}
SPACEGATE_IMAGE=${SPACEGATE_IMAGE}
SPACEGATE_ADMIN_IMAGE=${SPACEGATE_ADMIN_IMAGE}
AI_GATEWAY_SERVICE_IMAGE=${AI_GATEWAY_SERVICE_IMAGE}
HAI_PLUGIN_ARTIFACT=native-plugins/hai_hub_spacegate_plugins.so
WASM_ARTIFACT=wasm/spacegate_plugin_ai_gateway_queue.wasm
WASM_SOURCE_IN_IMAGE_PATH=/lib/spacegate/wasm/spacegate_plugin_ai_gateway_queue.wasm
WASM_SYSTEM_IMAGE_PATH=/lib/spacegate/wasm/spacegate_plugin_ai_gateway_queue.wasm
EOF

if [ -n "${WASM_SHA256:-}" ]; then
  cat >> "$ARTIFACT_DIR/IMAGES.txt" <<EOF
WASM_OCI_URL=oci://${WASM_OCI_REF}
WASM_SHA256=sha256:${WASM_SHA256}
EOF
fi

find "$ARTIFACT_DIR" -maxdepth 2 -type f | sort
cat "$ARTIFACT_DIR/SHA256SUMS"
cat "$ARTIFACT_DIR/IMAGES.txt"
```

最终交付目录：

```text
spacegate-workspace/image-artifacts/
  spacegate-<version>.tar
  spacegate-admin-<version>.tar
  ai-gateway-service-<version>.tar
  native-plugins/hai_hub_spacegate_plugins.so
  wasm/spacegate_plugin_ai_gateway_queue.wasm
  wasm/spacegate_plugin_ai_gateway_queue-<version>.wasm  # 可选，执行 16.6 后生成
  WASM-OCI.txt                                           # 可选，执行 16.6 后生成
  SHA256SUMS
  IMAGES.txt
```
