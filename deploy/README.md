# AI Gateway 队列限流 — 编译与部署指南

本文档说明如何编译 `ai-gateway-queue` Wasm 插件，并在 **本地开发 / Docker / Kubernetes** 等环境中部署，以及如何将 Wasm 发布为 **OCI 制品**。

相关文档：

- **生产 K8S 部署手册**：[`docs/k8s/production-deployment.md`](../docs/k8s/production-deployment.md)
- 插件行为与请求头：[`plugins/wasm/ai-gateway-queue/README.md`](../plugins/wasm/ai-gateway-queue/README.md)
- **AI Gateway 当前配置入口**：[`docs/ai-gateway/README.md`](../docs/ai-gateway/README.md)
- 测试用例规格：[`docs/ai-gateway/test-spec.md`](../docs/ai-gateway/test-spec.md)
- K8s manifest 目录：[`deploy/k8s/ai-gateway/`](k8s/ai-gateway/)

---

## 1. 架构概览

```text
Client
  → SpaceGate（ai-gateway-queue Wasm 插件）
  → ai-gateway-service（限流 / 入队 / wait / worker / 回调）
  → Redis 7+
  → 上游 LLM Service
```

| 组件 | 作用 |
|------|------|
| **ai-gateway-queue**（Wasm） | 解析 Policy / Tenant，调用后端限流，配额内转发上游，超额 429/202/wait |
| **ai-gateway-service** | 令牌桶、Redis Stream 队列、Worker、回调、指标 |
| **SpaceGate** | 加载 Wasm，路由到上游 |
| **Redis 7+** | 限流状态、队列、结果缓存 |

三种策略（`X-RateLimit-Policy`）均 **先过令牌桶**；配额内三种策略都直通上游；超额时：

- `abandon` → 429
- `queue` → 202 + 回调/轮询
- `wait` → 阻塞等待上游响应或 504

---

## 2. 编译 Wasm 插件

### 2.1 前置条件

- Rust 工具链（与 `spacegate` workspace 一致）
- 目标三元组 `wasm32-wasip1`

```bash
rustup target add wasm32-wasip1
```

### 2.2 Release 构建（部署用）

在 **`spacegate` 仓库根目录**执行：

```bash
cd spacegate

cargo build --release \
  --target wasm32-wasip1 \
  --manifest-path plugins/wasm/Cargo.toml \
  -p spacegate_plugin_ai_gateway_queue
```

产物路径：

```text
plugins/wasm/target/wasm32-wasip1/release/spacegate_plugin_ai_gateway_queue.wasm
```

### 2.3 Debug 构建（开发调试用）

```bash
cargo build \
  --target wasm32-wasip1 \
  --manifest-path plugins/wasm/Cargo.toml \
  -p spacegate_plugin_ai_gateway_queue
```

Debug 产物在 `plugins/wasm/target/wasm32-wasip1/debug/` 下，体积更大、未优化，**不要用于生产**。

### 2.4 校验产物

```bash
WASM=plugins/wasm/target/wasm32-wasip1/release/spacegate_plugin_ai_gateway_queue.wasm
file "$WASM"    # 应为 WebAssembly
ls -lh "$WASM"
shasum -a 256 "$WASM"
```

### 2.5 插件配置要点

Wasm 宿主侧需要完整 shell 配置（参考 [`.docker/ai-gateway-demo/plugin/wasm.ai-gateway-queue.json`](../../.docker/ai-gateway-demo/plugin/wasm.ai-gateway-queue.json)（工作区根目录）或 K8s `SgFilter`）：

| 字段 | 说明 |
|------|------|
| `url` | Wasm 来源：`file://`、`http(s)://` 或 `oci://` |
| `plugin_config.service_cluster` | 固定 cluster 名，如 `ai-gateway-service` |
| `clusters.ai-gateway-service` | 后端 base URL，如 `http://ai-gateway-service:18080` |
| `plugin_config.require_policy` | 是否强制 `X-RateLimit-Policy` |

**注意：** 插件不要在 Gateway 与 HTTPRoute **重复挂载**，否则会执行两次限流（双倍扣 token）。

---

## 3. 编译 ai-gateway-service（后端）

后端为普通 Rust 二进制，与 Wasm 分开构建。

### 3.1 本地运行

```bash
cd spacegate

cargo build --release -p ai-gateway-service

REDIS_URL=redis://127.0.0.1/ \
AI_UPSTREAM_BASE_URL=http://127.0.0.1:9000 \
AI_REQUIRE_HTTPS_CALLBACK=false \
./target/release/ai-gateway-service \
  --port 18080 \
  --host 127.0.0.1
```

配置模板：[`binary/ai-gateway-service/config/ai-gateway-service.example.toml`](../binary/ai-gateway-service/config/ai-gateway-service.example.toml)

### 3.2 构建 Linux 容器镜像（K8s / Docker）

```bash
cd spacegate/deploy/k8s/ai-gateway
./build-images.sh
# 默认镜像名 ai-gateway/service:dev
```

Dockerfile：[`resource/docker/ai-gateway-service/Dockerfile`](../resource/docker/ai-gateway-service/Dockerfile)

导入本地集群（示例 k3d）：

```bash
k3d image import ai-gateway/service:dev -c <cluster-name>
```

---

## 4. 本地开发部署（Cargo + 文件配置）

适合改代码、跑集成测试。

### 4.1 依赖服务

| 服务 | 端口 | 说明 |
|------|------|------|
| Redis 7+ | 6379 | 必须 |
| Mock 上游 | 9000 | 任意 HTTP 服务 |
| ai-gateway-service | 18080 | 队列后端 |
| SpaceGate | 9993 | 加载 Wasm + 路由 |

### 4.2 SpaceGate 文件配置

参考 [`resource/ai-gateway-demo/`](../resource/ai-gateway-demo/) 模板，复制到 **工作区根目录** `.docker/ai-gateway-demo/`（与 `spacegate` 仓库同级，非 spacegate 子目录）：

```text
ai-gateway-dev/.docker/ai-gateway-demo/
  config.json
  gateway/ai-demo/
  plugin/wasm.ai-gateway-queue.json    # 仅 JSON
  plugins/spacegate_plugin_ai_gateway_queue.wasm
```

`resource/ai-gateway-demo/plugin/wasm.ai-gateway-queue.json` 内含本机绝对路径，**不要直接用于 Docker**；请使用 `.docker` 下已改为 `file:///etc/spacegate/plugins/...` 的版本。

`wasm.ai-gateway-queue.json` 中 `clusters` 示例：

```json
"clusters": {
  "ai-gateway-service": "http://127.0.0.1:18080"
}
```

### 4.3 启动 SpaceGate（示例）

```bash
cd spacegate
cargo run -p spacegate -- -c file:resource/ai-gateway-demo
# Docker 使用工作区根目录 .docker/ai-gateway-demo（挂载到 /etc/spacegate）
```

**避免** 本地 debug SpaceGate 与 Docker 容器 **同时占用 `:9993`**。

### 4.4 冒烟测试

```bash
# 经网关（插件生效）
curl -i http://127.0.0.1:9993/v1/chat/completions \
  -H 'X-RateLimit-Policy: abandon' \
  -H 'X-Tenant-Id: demo' \
  -H 'Content-Type: application/json' \
  -d '{"prompt":"hello"}'

# 直连后端
curl http://127.0.0.1:18080/healthz
```

### 4.5 自动化测试

```bash
cd spacegate

# 单元测试
cargo test -p ai-gateway-service --lib

# 集成测试（需 Redis）
./binary/ai-gateway-service/scripts/run-integration-tests.sh

# Wasm 策略逻辑
./binary/ai-gateway-service/scripts/run-wasm-policy-tests.sh
```

---

## 5. Docker Compose 部署

> 若工作区根目录的 `docker-compose.yml` 已删除，可从 Git 历史恢复，或参照本节手工起容器。

典型栈：

| 容器 | 端口 | 镜像 |
|------|------|------|
| ai-gateway-redis | 6379 | redis:7 |
| ai-gateway-service | 18080 | ai-gateway/service:dev |
| ai-gateway-spacegate | 9993 | spacegate + Wasm 挂载 |
| ai-gateway-web | 9080 | 管理前端 |
| ai-gateway-mock-upstream | 9000 | mock LLM |

要点：

- 配置目录挂载：**工作区根目录** `.docker/ai-gateway-demo/` → 容器内 `/etc/spacegate`
- **admin-server 卷须可写**（勿 `:ro`），否则管理界面保存插件报 `Read-only file system`
- Wasm 放在 `plugin/`（JSON）与 `plugins/`（`.wasm` 二进制），**勿**把 `.wasm` 放进 `plugin/`（会被当 JSON 解析导致 SpaceGate 启动失败）
- macOS 上 **不能** `docker cp` 本机编译的 Mach-O 二进制进 Linux 容器，需在 Linux 环境构建镜像

管理界面 `:9080` 依赖 admin-server 能读到 `/etc/spacegate` 配置；若报 `No such file or directory`，检查 volume 挂载是否存在。

---

## 6. Kubernetes 部署

Manifest 位于 [`deploy/k8s/ai-gateway/`](k8s/ai-gateway/)。

### 6.1 前置：安装 SpaceGate 基础组件

```bash
# Gateway API CRD（见 docs/k8s/installation.md）
kubectl apply -f https://github.com/kubernetes-sigs/gateway-api/releases/download/v0.6.2/standard-install.yaml

kubectl apply -f resource/kube-manifests/namespace.yaml
kubectl apply -f resource/kube-manifests/gatewayclass.yaml
kubectl apply -f resource/kube-manifests/spacegate-httproute.yaml
kubectl apply -f resource/kube-manifests/spacegate-mcproute.yaml
kubectl apply -f resource/kube-manifests/spacegate-gateway.yaml
kubectl apply -f resource/kube-manifests/higress-wasmplugin-crd.yaml
```

SpaceGate DaemonSet 使用 `CONFIG=k8s:spacegate`，监听同 namespace 下的 Gateway / HTTPRoute / HTTPSpaceroute / MCPRoute / SgFilter / WasmPlugin。以上 CRD 必须先于 DaemonSet 安装；缺失后 watcher 会退出，补装 CRD 后需执行 `kubectl rollout restart daemonset/spacegate -n spacegate`。

### 6.2 一键部署 AI Gateway 栈

```bash
# 1. 构建并导入 ai-gateway-service 镜像
cd deploy/k8s/ai-gateway
./build-images.sh
k3d image import ai-gateway/service:dev -c <cluster>   # 按需

# 2. 编译 Wasm + apply
./apply.sh

# 3. 验证
./verify.sh
```

`apply.sh` 会：

1. 编译 `spacegate_plugin_ai_gateway_queue.wasm`
2. 写入 `files/` 供 Kustomize 生成 ConfigMap
3. `kubectl apply -k .` 部署 Redis、mock-upstream、wasm-server、ai-gateway-service、Gateway、HTTPRoute、SgFilter

### 6.3 资源说明

| 资源 | 说明 |
|------|------|
| `ai-gateway-redis` | Redis 7 |
| `ai-gateway-service` | 队列/限流后端 Service `:18080` |
| `ai-gateway-wasm` | Nginx 通过 HTTP 分发 `.wasm`（免改 SpaceGate DaemonSet） |
| `ai-gateway` Gateway | 监听 `:9993` |
| `ai-api` HTTPRoute | `/v1/*` → mock-upstream |
| `SgFilter ai-gateway-queue` | Wasm 插件 + `clusters` 映射（**推荐**） |

### 6.4 Wasm 插件在 K8s 上的两种挂载方式

#### 方式 A：SgFilter（推荐）

完整 shell spec 含 `clusters`，见 [`sgfilter-ai-gateway-queue.yaml`](k8s/ai-gateway/sgfilter-ai-gateway-queue.yaml)：

```yaml
config:
  url: http://ai-gateway-wasm/spacegate_plugin_ai_gateway_queue.wasm
  clusters:
    ai-gateway-service: http://ai-gateway-service:18080
  plugin_config:
    service_cluster: ai-gateway-service
    require_policy: true
    # ...
```

#### 方式 B：Higress WasmPlugin

[`wasmplugin-ai-gateway-queue.yaml`](k8s/ai-gateway/wasmplugin-ai-gateway-queue.yaml) 中 `defaultConfig` **不会**自动写入顶层 `clusters`，生产环境需配合 SgFilter 或扩展 CRD 转换逻辑。

私有 OCI 仓库需配置 `imagePullSecret`。

### 6.5 网关入口测试

SpaceGate 使用 `hostNetwork` 时，在节点上访问：

```bash
curl -i http://<node-ip>:9993/v1/chat/completions \
  -H 'X-RateLimit-Policy: abandon' \
  -H 'X-Tenant-Id: demo' \
  -d '{"prompt":"hi"}'
```

### 6.6 生产替换清单

| 开发默认 | 生产建议 |
|----------|----------|
| mock-upstream | 真实 LLM Service |
| `AI_REQUIRE_HTTPS_CALLBACK=false` | `true`，回调 URL 必须 HTTPS |
| HTTP Wasm 分发 | OCI 制品 + `oci://` URL |
| 单副本 Redis | 托管 Redis / Sentinel / Cluster |
| 无对象存储 | 配置 S3/MinIO（大 body offload） |

---

## 7. 制作 OCI 制品

SpaceGate 支持从 OCI 仓库拉取 Wasm，URL 形式：

```text
oci://<registry>/<repository>:<tag>
docker://...    # 等价
image://...     # 等价
oci+http://...  # 本地非 TLS registry
```

接受的 layer 媒体类型：

- `application/vnd.module.wasm.content.layer.v1+wasm`（推荐）
- `application/vnd.wasm.content.layer.v1+wasm`
- `application/wasm`

### 7.1 安装 ORAS

```bash
brew install oras
# 或从 https://github.com/oras-project/oras/releases 下载
```

### 7.2 编译并计算 digest

```bash
cd spacegate

cargo build --release \
  --target wasm32-wasip1 \
  --manifest-path plugins/wasm/Cargo.toml \
  -p spacegate_plugin_ai_gateway_queue

WASM=plugins/wasm/target/wasm32-wasip1/release/spacegate_plugin_ai_gateway_queue.wasm
shasum -a 256 "$WASM"
```

### 7.3 推送到仓库

```bash
# 登录（按仓库类型选择）
oras login ghcr.io -u YOUR_USER
# oras login registry.cn-hangzhou.aliyuncs.com
# oras login your-harbor.example.com

REGISTRY=ghcr.io/your-org
TAG=v1.0.0

oras push "${REGISTRY}/ai-gateway-queue:${TAG}" \
  --artifact-type application/vnd.module.wasm.content.layer.v1+wasm \
  "${WASM}:application/wasm"
```

推送成功后配置：

```yaml
url: oci://ghcr.io/your-org/ai-gateway-queue:v1.0.0
sha256: sha256:<上一步 shasum 输出>   # 可选，建议生产开启
```

在 SgFilter / WasmPlugin 中替换 `url` 即可；私有仓库配合 `imagePullSecret`。

### 7.4 本地 Registry 测试

```bash
docker run -d -p 5000:5000 --name registry registry:2

oras push localhost:5000/ai-gateway-queue:v1 \
  --artifact-type application/vnd.module.wasm.content.layer.v1+wasm \
  "${WASM}:application/wasm"
```

SpaceGate 配置（本地/insecure）：

```text
oci+http://localhost:5000/ai-gateway-queue:v1
```

### 7.5 OCI 注意事项

| 项 | 说明 |
|----|------|
| Docker Hub | 通常 **不支持** Wasm OCI Artifact，请用 GHCR / Harbor / ACR / ECR 等 |
| 与容器镜像区别 | OCI Artifact 是单层 Wasm 文件，不是 `docker build` 的应用镜像 |
| ai-gateway-service 镜像 | 仍用 [`build-images.sh`](k8s/ai-gateway/build-images.sh) 单独构建 |
| 版本更新 | 改 tag 重新 push；或在配置中更新 `sha256` / `module_cache_key` 触发重新拉取 |

### 7.6 一键推送脚本（可选）

```bash
#!/usr/bin/env bash
set -euo pipefail
REGISTRY="${REGISTRY:?set REGISTRY e.g. ghcr.io/your-org}"
TAG="${TAG:-v1.0.0}"
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
WASM="$ROOT/plugins/wasm/target/wasm32-wasip1/release/spacegate_plugin_ai_gateway_queue.wasm"

cd "$ROOT"
cargo build --release --target wasm32-wasip1 \
  --manifest-path plugins/wasm/Cargo.toml \
  -p spacegate_plugin_ai_gateway_queue

DIGEST=$(shasum -a 256 "$WASM" | awk '{print $1}')

oras push "${REGISTRY}/ai-gateway-queue:${TAG}" \
  --artifact-type application/vnd.module.wasm.content.layer.v1+wasm \
  "${WASM}:application/wasm"

echo "url:      oci://${REGISTRY}/ai-gateway-queue:${TAG}"
echo "sha256:   sha256:${DIGEST}"
```

保存为 [`deploy/push-wasm-oci.sh`](push-wasm-oci.sh)（脚本内 `ROOT` 指向 `spacegate` 仓库根目录）后：

```bash
REGISTRY=ghcr.io/your-org TAG=v1.0.0 ./deploy/push-wasm-oci.sh
```

---

## 8. 各环境对照表

| 环境 | Wasm 分发 | 后端地址配置 | 入口 |
|------|-----------|--------------|------|
| 本地 Cargo | `file://.../plugins/*.wasm` | `127.0.0.1:18080` | `:9993` SpaceGate |
| Docker | volume 挂载 `plugins/` | `http://ai-gateway-service:18080` | `:9993` / `:9080` 管理端 |
| K8s（HTTP） | `http://ai-gateway-wasm/...` | `http://ai-gateway-service:18080` | Gateway `:9993` |
| K8s / 生产（OCI） | `oci://registry/...:tag` | K8s Service DNS | Gateway `:9993` |

---

## 9. 常见问题

**Q: 第一次请求就 429？**  
A: 检查插件是否在 Gateway 与 Route **重复挂载**；或测试租户 burst 过小。Admin 设置：`PUT /v1/admin/tenant-rate-limits`。

**Q: `:9080` 报 `No such file or directory`？**  
A: admin-server 读不到 `/etc/spacegate` 配置，恢复 **工作区根目录** `.docker/ai-gateway-demo` 挂载。

**Q: SpaceGate 启动报 JSON parse error？**  
A: `plugin/` 目录下有 `.wasm` 文件，应移到 `plugins/` 子目录。

**Q: macOS 二进制拷进 Linux 容器失败？**  
A: 在 Linux 环境 `docker build` 或使用已构建的 `ai-gateway/service:dev` 镜像。

**Q: WasmPlugin 无法连 ai-gateway-service？**  
A: Higress WasmPlugin 的 `defaultConfig` 不含 `clusters`，请用 **SgFilter** 或改用 OCI + 完整 spec。

---

## 10. 目录索引

```text
spacegate/
├── plugins/wasm/ai-gateway-queue/     # Wasm 插件源码
├── binary/ai-gateway-service/         # 队列/限流后端
├── resource/ai-gateway-demo/          # 文件模式配置模板
├── deploy/
│   ├── README.md                      # 本文档
│   └── k8s/ai-gateway/                # K8s manifest + apply.sh
└── docs/
    ├── ai-gateway/                    # 当前 AI Gateway 文档
    ├── k8s/                            # K8s 安装与部署手册
    └── archive/                        # 阶段性评审与历史流程
```
