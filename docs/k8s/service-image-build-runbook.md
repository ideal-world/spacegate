# SpaceGate 拆分服务镜像构建 Runbook

本文档用于从 `spacegate-workspace` 工作区直接构建拆分部署所需的三个服务镜像，并将镜像制品打包成 tar 包，便于通过 U 盘复制到其他服务器。

1. `spacegate`：网关运行时镜像，包含 K8s、Wasm、native dylib 支持。
2. `spacegate-admin`：网关配置管理镜像，包含 Admin 前端静态资源和 Admin 后端。
3. `ai-gateway-service`：AI Gateway 排队、限流、wait、worker、回调服务镜像。

> 本文不使用 `Dockerfile.all-in-one`。生产部署应保持三个服务独立构建、独立发布、独立回滚。本文主流程不依赖镜像仓库；如果目标环境有 registry，可额外执行文末的 `docker push`。

## 1. 准备变量

执行目录：`spacegate-workspace`

执行脚本：以下命令块用于设置后续所有构建命令共享的路径、镜像名和离线制品目录。

```bash
# 此处只是示例，根据实际项目路径设置
cd /path/to/spacegate-workspace

export WORKSPACE_ROOT="$PWD"
export SPACEGATE_ROOT="$WORKSPACE_ROOT/spacegate"
export ADMIN_FE_ROOT="$WORKSPACE_ROOT/spacegate-admin-fe"
export ADMIN_DOCKER_CONTEXT="$SPACEGATE_ROOT"
export ADMIN_DOCKER_DIR="$SPACEGATE_ROOT/resource/docker/spacegate-admin"
export ARTIFACT_DIR="$WORKSPACE_ROOT/image-artifacts"

# VERSION 建议使用 Git tag、日期版本或交付版本号，例如 20260709-001。
export VERSION="1.0.3"

export SPACEGATE_IMAGE="spacegate:$VERSION"
export SPACEGATE_ADMIN_IMAGE="spacegate-admin:$VERSION"
export AI_GATEWAY_SERVICE_IMAGE="ai-gateway-service:$VERSION"

# Admin 合并镜像的 Nginx 基础镜像。若默认镜像源不可用，可改成本地已导入的 nginx Debian 镜像。
export ADMIN_NGINX_IMAGE="nginx:1.27-bookworm"

# HAI 静态插件源码所在的 hai-hub 仓库；网关镜像构建会从此目录编译它。
export HAI_HUB_ROOT="/path/to/hai-hub"

mkdir -p "$ARTIFACT_DIR"
```

检查必要目录：

```bash
test -d "$SPACEGATE_ROOT"
test -d "$ADMIN_FE_ROOT"
test -d "$HAI_HUB_ROOT"
test -f "$HAI_HUB_ROOT/backend/hai-hub-spacegate-plugins/Cargo.toml"
test -f "$SPACEGATE_ROOT/resource/docker/spacegate-k8s/Dockerfile"
test -f "$ADMIN_DOCKER_DIR/Dockerfile"
test -f "$SPACEGATE_ROOT/resource/docker/ai-gateway-service/Dockerfile"
```

检查 Admin Nginx 基础镜像是否已在本机 Docker 中。离线打包场景不要依赖构建时再访问 Docker Hub：

```bash
docker image inspect "$ADMIN_NGINX_IMAGE" >/dev/null
```

如果本机没有这个镜像，需要先在有网络的机器准备基础镜像 tar：

```bash
docker pull nginx:1.27-bookworm
docker save nginx:1.27-bookworm -o nginx-1.27-bookworm.tar
```

复制到构建机后导入：

```bash
docker load -i nginx-1.27-bookworm.tar
docker image inspect "$ADMIN_NGINX_IMAGE" >/dev/null
```

## 2. 推荐：逐步复制执行主流程

执行目录：`spacegate-workspace`

执行方式：先在同一 shell 完成第 1 节变量设置，然后按本节代码块从上到下一步一步执行。Admin 构建命令会再次校验其 Docker context；如果某一步失败，先停在当前步骤看完整错误，不要继续执行后续步骤。

### 2.1 确认 Admin Nginx 基础镜像已经在本机

```bash
cd "$WORKSPACE_ROOT"
docker image inspect "$ADMIN_NGINX_IMAGE" >/dev/null
```

如果失败，先导入离线基础镜像：

```bash
cd "$WORKSPACE_ROOT"
docker load -i nginx-1.27-bookworm.tar
docker image inspect "$ADMIN_NGINX_IMAGE" >/dev/null
```

### HAI 静态构建一致性检查

在构建镜像前，可运行以下检查确认 all-in-one 与 K8s Dockerfile 都从 `HAI_HUB_ROOT` 静态编译 `hai-hub-spacegate`，且未恢复过期的 HAI dylib 路径：

```bash
cd "$WORKSPACE_ROOT"
sh docker/all-in-one/tests/test-hai-plugin-build.sh
```

该检查不构建镜像；all-in-one 仅作为本地集成验证，生产仍使用本 runbook 的三个独立服务镜像。

### 2.2 构建 SpaceGate 网关镜像

```bash
cd "$WORKSPACE_ROOT"

docker build --progress=plain \
  --build-context "hai_hub=$HAI_HUB_ROOT" \
  -f "$SPACEGATE_ROOT/resource/docker/spacegate-k8s/Dockerfile" \
  -t "$SPACEGATE_IMAGE" \
  "$SPACEGATE_ROOT"
```

如果这里失败，重点看 `cargo build --manifest-path /hai-hub/Cargo.toml --release -p hai-hub-spacegate` 上方的 Rust 编译错误；Docker 最后一行 `exit code: 101` 只是汇总错误。

### 2.3 验证 HAI 静态插件已内置

```bash
cd "$WORKSPACE_ROOT"

docker run --rm --entrypoint sh "$SPACEGATE_IMAGE" -c '
  set -e
  test -x /usr/local/bin/spacegate
  /usr/local/bin/spacegate --help >/dev/null
'
```

HAI 插件已静态链接到 `/usr/local/bin/spacegate`，不会生成或导出单独的 HAI `.so`。`/lib/spacegate/plugins` 只保留给可选的第三方 native dylib。

### 2.4 构建 Admin SDK 和前端静态资源

```bash
cd "$SPACEGATE_ROOT/sdk/admin-client"
npm ci
npm run build
```

```bash
cd "$ADMIN_FE_ROOT"
npm ci
npm run build
```

Vite 输出 `Some chunks are larger than 500 kB after minification` 是体积告警，不是构建失败；只要最后出现 `built` 并且命令返回成功，就可以继续。

### 2.5 准备 Admin 镜像构建上下文

```bash
cd "$WORKSPACE_ROOT"

: "${ADMIN_DOCKER_CONTEXT:?请先执行第 1 节变量设置}"
test -d "$ADMIN_DOCKER_CONTEXT"
test -f "$ADMIN_DOCKER_DIR/Dockerfile"
test -d "$ADMIN_FE_ROOT/dist"

rsync -a --delete "$ADMIN_FE_ROOT/dist/" "$ADMIN_DOCKER_DIR/dist/"
test -f "$ADMIN_DOCKER_DIR/dist/index.html"
```

### 2.6 构建 SpaceGate Admin 前后端合并镜像

```bash
cd "$WORKSPACE_ROOT"

: "${ADMIN_DOCKER_CONTEXT:?请先执行第 1 节变量设置}"
test -f "$ADMIN_DOCKER_DIR/Dockerfile"
test -f "$ADMIN_DOCKER_DIR/dist/index.html"

docker build --progress=plain \
  --build-context "spacegate_src=$SPACEGATE_ROOT" \
  --build-arg "NGINX_IMAGE=$ADMIN_NGINX_IMAGE" \
  -f "$ADMIN_DOCKER_DIR/Dockerfile" \
  -t "$SPACEGATE_ADMIN_IMAGE" \
  "$ADMIN_DOCKER_CONTEXT"
```

如果这里出现 `nginx:... failed to resolve source metadata`，说明 Docker 当前镜像源无法拉取 Nginx 基础镜像；回到 2.1 先导入或替换 `ADMIN_NGINX_IMAGE`。

### 2.7 构建 AI Gateway Service 镜像

```bash
cd "$WORKSPACE_ROOT"

docker build --progress=plain \
  -f "$SPACEGATE_ROOT/resource/docker/ai-gateway-service/Dockerfile" \
  -t "$AI_GATEWAY_SERVICE_IMAGE" \
  "$SPACEGATE_ROOT"
```

### 2.8 检查三个镜像都已经生成

```bash
docker image inspect "$SPACEGATE_IMAGE" >/dev/null
docker image inspect "$SPACEGATE_ADMIN_IMAGE" >/dev/null
docker image inspect "$AI_GATEWAY_SERVICE_IMAGE" >/dev/null

docker images | grep -E 'spacegate|spacegate-admin|ai-gateway-service'
```

### 2.9 保存离线 tar 包和校验文件

```bash
cd "$WORKSPACE_ROOT"
mkdir -p "$ARTIFACT_DIR"

docker save "$SPACEGATE_IMAGE" \
  -o "$ARTIFACT_DIR/spacegate-${VERSION}.tar"

docker save "$SPACEGATE_ADMIN_IMAGE" \
  -o "$ARTIFACT_DIR/spacegate-admin-${VERSION}.tar"

docker save "$AI_GATEWAY_SERVICE_IMAGE" \
  -o "$ARTIFACT_DIR/ai-gateway-service-${VERSION}.tar"

shasum -a 256 "$ARTIFACT_DIR"/*.tar > "$ARTIFACT_DIR/SHA256SUMS"

ls -lh "$ARTIFACT_DIR"
cat "$ARTIFACT_DIR/SHA256SUMS"
```

最终把整个目录复制到 U 盘：

```text
spacegate-workspace/image-artifacts/
  spacegate-<version>.tar
  spacegate-admin-<version>.tar
  ai-gateway-service-<version>.tar
  SHA256SUMS
```

第 3～7 节是上面每一步的细节说明和排查命令。当前建议优先使用本节逐步流程；一键脚本放在第 12 节作为可选项，等逐步流程稳定后再使用。

## 3. 构建 SpaceGate 网关镜像

执行目录：`spacegate-workspace`

执行脚本：以下命令块调用 `spacegate/resource/docker/spacegate-k8s/Dockerfile` 构建网关镜像。

该镜像会编译：

```bash
cd "$SPACEGATE_ROOT"
cargo build --release -p spacegate --features build-k8s,wasm,dylib,static-openssl
```

### 3.1 HAI 静态插件构建和 copy 逻辑

`hai-hub-spacegate` 不在 `spacegate` 仓库内，它由 `HAI_HUB_ROOT` 指向的 `hai-hub` 仓库提供。Dockerfile 将它作为 Linux 可执行文件构建，并复制为最终镜像的 `/usr/local/bin/spacegate`。HAI 插件在该二进制中静态注册，不会生成单独的 HAI `.so` 制品。

当前静态注册的 HAI 插件包括：

| 插件 code | 插件类型 | 作用 |
| --- | --- | --- |
| `hub-request-id` | `RequestIdPlugin` | 为请求注入或透传请求 ID |
| `auth` | `AuthPlugin` | 通用鉴权插件 |
| `hai-observe` | `HaiObservePlugin` | HAI 调用审计、指标与链路字段采集 |
| `hai-auth` | `HaiAuthPlugin` | 基于 HAI API Key 和资产订阅鉴权 |
| `hai-asset` | `HaiAssetPlugin` | 从 Redis 加载并校验 HAI 资产 |
| `hai-quota` | `HaiQuotaPlugin` | 基于资产配置做 QPS/并发限流 |
| `hai-dispatch` | `HaiDispatchPlugin` | 根据资产运行时配置派发上游 |

更新 `hai-hub` 中的 HAI 插件代码后，必须使用新的 `HAI_HUB_ROOT` 重建网关镜像并滚动更新 K8s 工作负载；运行时不能热加载新的 HAI 代码。

当前网关镜像 Dockerfile 通过 BuildKit external build context 引入 `hai-hub`：

```bash
docker build \
  --build-context hai_hub="$HAI_HUB_ROOT" \
  -f "$SPACEGATE_ROOT/resource/docker/spacegate-k8s/Dockerfile" \
  -t "$SPACEGATE_IMAGE" \
  "$SPACEGATE_ROOT"
```

Dockerfile 内部关键片段：

```dockerfile
COPY --from=hai_hub . /hai-hub

RUN mkdir -p /hai-hub/.cargo
RUN sed -i '/"backend\/hai-hub-resource"/d;/"services\/hai-hub-all"/d;/"services\/hai-hub-auth-plugin"/d' /hai-hub/Cargo.toml
RUN printf '%s\n' \
    '[patch."https://github.com/ideal-world/spacegate"]' \
    'spacegate-config = { path = "/app/crates/config" }' \
    'spacegate-ext-axum = { path = "/app/crates/extension/axum" }' \
    'spacegate-ext-redis = { path = "/app/crates/extension/redis" }' \
    'spacegate-kernel = { path = "/app/crates/kernel" }' \
    'spacegate-model = { path = "/app/crates/model" }' \
    'spacegate-plugin = { path = "/app/crates/plugin" }' \
    'spacegate-shell = { path = "/app/crates/shell" }' \
    > /hai-hub/.cargo/config.toml
WORKDIR /hai-hub
RUN --mount=type=cache,target=/usr/local/cargo/registry \
    --mount=type=cache,target=/usr/local/cargo/git \
    --mount=type=cache,target=/hai-hub/target \
    cargo build --manifest-path /hai-hub/Cargo.toml --release -p hai-hub-spacegate --config /hai-hub/.cargo/config.toml \
    && install -Dm755 target/release/hai-hub-spacegate /hai-plugin/hai-hub-spacegate
```

这里的 `sed` 只修改 Docker build 中的 `/hai-hub` 临时副本，不会修改本地 `HAI_HUB_ROOT`。原因是 `hai-hub` workspace 还有其他服务 member，这些 member 可能依赖构建上下文之外的兄弟仓库；镜像只需要静态 HAI 网关成员 `hai-hub-spacegate`。

不要在宿主机单独构建或归档 HAI `.so`：macOS 产物无法用于 Linux，而当前 K8s 运行方式也不加载 HAI dylib。需要更新 HAI 代码时，始终通过本节 Dockerfile 从 `HAI_HUB_ROOT` 构建 Linux 网关镜像。

### 3.2 构建网关镜像

执行构建时回到工作区根目录，便于统一引用 `SPACEGATE_ROOT` 和 `HAI_HUB_ROOT`：

```bash
cd "$WORKSPACE_ROOT"

docker build \
  --build-context hai_hub="$HAI_HUB_ROOT" \
  -f "$SPACEGATE_ROOT/resource/docker/spacegate-k8s/Dockerfile" \
  -t "$SPACEGATE_IMAGE" \
  "$SPACEGATE_ROOT"
```

快速检查镜像内是否包含静态 HAI 网关二进制：

```bash
docker run --rm --entrypoint sh "$SPACEGATE_IMAGE" -c '
  set -e
  test -x /usr/local/bin/spacegate
  /usr/local/bin/spacegate --help >/dev/null
'
```

`/lib/spacegate/plugins` 仍是可选第三方 native dylib 的加载目录，但不是 HAI 插件的来源。不要通过 volume 覆盖它来更新 HAI；更新 HAI 必须替换完整的网关镜像。

## 4. 构建 SpaceGate Admin 前后端合并镜像

执行目录：`spacegate-workspace`

执行脚本：本节先在 `spacegate-admin-fe` 构建前端静态资源，再使用 `spacegate/resource/docker/spacegate-admin/Dockerfile` 组装合并镜像。Admin 后端二进制会在 Docker builder 阶段构建为 Linux 产物，不使用宿主机二进制。

当前合并镜像使用：

```text
spacegate/resource/docker/spacegate-admin/Dockerfile
```

镜像内包含：

- Nginx：监听 `9080`，提供前端静态资源。
- Admin Server：由 `start.sh` 拉起，监听 `9081`。
- Nginx `/api` 反向代理到本机 `9081`。

### 4.1 构建 Admin 前端静态资源

执行目录：先进入 `spacegate/sdk/admin-client`，再进入 `spacegate-admin-fe`

前端依赖工作区内的 `spacegate/sdk/admin-client` 和 `spacegate-admin-front`，先确认目录存在：

```bash
test -d "$SPACEGATE_ROOT/sdk/admin-client"
test -d "$WORKSPACE_ROOT/spacegate-admin-front"
```

构建 SDK 和前端：

```bash
cd "$SPACEGATE_ROOT/sdk/admin-client"
npm ci
npm run build

cd "$ADMIN_FE_ROOT"
npm ci
npm run build
```

### 4.2 准备 Docker build context

执行目录：`spacegate-workspace`

```bash
cd "$WORKSPACE_ROOT"

export ADMIN_DOCKER_CONTEXT=""
export ADMIN_DOCKER_DIR="/resource/docker/spacegate-admin"

rsync -a --delete "$ADMIN_FE_ROOT/dist/" "$ADMIN_DOCKER_DIR/dist/"
```

### 4.3 构建合并镜像

执行目录：`spacegate-workspace`

```bash
cd "$WORKSPACE_ROOT"

: "${ADMIN_DOCKER_CONTEXT:?请先执行第 1 节变量设置}"
test -f "$ADMIN_DOCKER_DIR/Dockerfile"
test -f "$ADMIN_DOCKER_DIR/dist/index.html"

docker build \
  --build-context "spacegate_src=$SPACEGATE_ROOT" \
  --build-arg "NGINX_IMAGE=$ADMIN_NGINX_IMAGE" \
  -f "$ADMIN_DOCKER_DIR/Dockerfile" \
  -t "$SPACEGATE_ADMIN_IMAGE" \
  "$ADMIN_DOCKER_CONTEXT"
```

如果这里出现 `nginx:... failed to resolve source metadata`，说明 Docker 当前镜像源无法拉取 Nginx 基础镜像。处理方式：

```bash
# 方式一：先离线导入或手工拉取，成功后重跑构建脚本。
docker load -i nginx-1.27-bookworm.tar
# 或 docker pull "$ADMIN_NGINX_IMAGE"

# 方式二：如果目标环境已有内网 Nginx Debian 镜像，改用本地/内网镜像。
export ADMIN_NGINX_IMAGE="<your-local-or-internal-nginx-bookworm-image>"
```

不要使用 Alpine 版 Nginx 作为默认值；Admin Server 是 glibc Linux 二进制，Debian/Bookworm 运行层更匹配。

快速检查镜像：

```bash
docker run --rm --entrypoint sh "$SPACEGATE_ADMIN_IMAGE" -c '
  set -e
  test -x /usr/src/app/admin-server
  test -f /usr/share/nginx/html/index.html
  nginx -t
'
```

K8s 中运行该镜像时，需要设置：

```yaml
- name: CONFIG
  value: k8s:spacegate
```

容器默认暴露 `9080`，前端访问 `/api` 会代理到容器内 Admin Server。该合并镜像对应的基础清单是 `resource/kube-manifests/spacegate-admin-server.yaml`；`deploy/k8s/ai-gateway/admin-ui.yaml` 仍是旧的前后端拆分示例，不适用于本节的合并镜像。

## 5. 构建 AI Gateway Service 镜像

执行目录：`spacegate-workspace`

执行脚本：以下命令块调用 `spacegate/resource/docker/ai-gateway-service/Dockerfile` 构建排队限流服务镜像。

当前镜像使用：

```text
spacegate/resource/docker/ai-gateway-service/Dockerfile
```

构建命令：

```bash
cd "$WORKSPACE_ROOT"

docker build \
  -f "$SPACEGATE_ROOT/resource/docker/ai-gateway-service/Dockerfile" \
  -t "$AI_GATEWAY_SERVICE_IMAGE" \
  "$SPACEGATE_ROOT"
```

快速检查镜像：

```bash
docker run --rm --entrypoint sh "$AI_GATEWAY_SERVICE_IMAGE" -c '
  set -e
  test -x /usr/local/bin/ai-gateway-service
  /usr/local/bin/ai-gateway-service --help >/tmp/ai-gateway-service-help.txt
'
```

运行时至少需要配置 Redis 和上游地址，可通过环境变量或 ConfigMap 注入：

```yaml
- name: REDIS_URL
  value: redis://ai-gateway-redis:6379
- name: AI_UPSTREAM_BASE_URL
  value: http://your-upstream-service:port
```

## 6. 手动一次性构建全部镜像

确认第 1 节变量已经设置后，可直接执行：

```bash
cd "$WORKSPACE_ROOT"

docker build \
  --build-context hai_hub="$HAI_HUB_ROOT" \
  -f "$SPACEGATE_ROOT/resource/docker/spacegate-k8s/Dockerfile" \
  -t "$SPACEGATE_IMAGE" \
  "$SPACEGATE_ROOT"

cd "$SPACEGATE_ROOT/sdk/admin-client"
npm ci
npm run build

cd "$ADMIN_FE_ROOT"
npm ci
npm run build

: "${ADMIN_DOCKER_CONTEXT:?请先执行第 1 节变量设置}"
test -f "$ADMIN_DOCKER_DIR/Dockerfile"
rsync -a --delete "$ADMIN_FE_ROOT/dist/" "$ADMIN_DOCKER_DIR/dist/"
test -f "$ADMIN_DOCKER_DIR/dist/index.html"

docker build \
  --build-context "spacegate_src=$SPACEGATE_ROOT" \
  --build-arg "NGINX_IMAGE=$ADMIN_NGINX_IMAGE" \
  -f "$ADMIN_DOCKER_DIR/Dockerfile" \
  -t "$SPACEGATE_ADMIN_IMAGE" \
  "$ADMIN_DOCKER_CONTEXT"

docker build \
  -f "$SPACEGATE_ROOT/resource/docker/ai-gateway-service/Dockerfile" \
  -t "$AI_GATEWAY_SERVICE_IMAGE" \
  "$SPACEGATE_ROOT"
```

## 7. 将镜像保存为离线 tar 包

执行目录：`spacegate-workspace`

执行脚本：以下命令块会把本机 Docker 中的三个镜像保存成 tar 包，并额外生成 SHA-256 校验文件。

```bash
cd "$WORKSPACE_ROOT"
mkdir -p "$ARTIFACT_DIR"

docker save "$SPACEGATE_IMAGE" \
  -o "$ARTIFACT_DIR/spacegate-${VERSION}.tar"

docker save "$SPACEGATE_ADMIN_IMAGE" \
  -o "$ARTIFACT_DIR/spacegate-admin-${VERSION}.tar"

docker save "$AI_GATEWAY_SERVICE_IMAGE" \
  -o "$ARTIFACT_DIR/ai-gateway-service-${VERSION}.tar"

shasum -a 256 "$ARTIFACT_DIR"/*.tar > "$ARTIFACT_DIR/SHA256SUMS"

ls -lh "$ARTIFACT_DIR"
cat "$ARTIFACT_DIR/SHA256SUMS"
```

离线交付目录：

```text
spacegate-workspace/image-artifacts/
  spacegate-<version>.tar
  spacegate-admin-<version>.tar
  ai-gateway-service-<version>.tar
  SHA256SUMS
```

将 `image-artifacts/` 整个目录复制到 U 盘。

## 8. 在目标服务器导入离线镜像

执行目录：目标服务器上 U 盘挂载目录或复制后的制品目录。

执行脚本：以下命令块校验 tar 包后导入 Docker 镜像。

```bash
cd /path/to/image-artifacts

export VERSION="<version>"
export SPACEGATE_IMAGE="spacegate:$VERSION"
export SPACEGATE_ADMIN_IMAGE="spacegate-admin:$VERSION"
export AI_GATEWAY_SERVICE_IMAGE="ai-gateway-service:$VERSION"

shasum -a 256 -c SHA256SUMS

docker load -i "spacegate-${VERSION}.tar"
docker load -i "spacegate-admin-${VERSION}.tar"
docker load -i "ai-gateway-service-${VERSION}.tar"

docker images | grep -E 'spacegate|spacegate-admin|ai-gateway-service'
```

如果目标服务器使用 containerd 而不是 Docker，可使用：

```bash
cd /path/to/image-artifacts

export VERSION="<version>"

ctr -n k8s.io images import "spacegate-${VERSION}.tar"
ctr -n k8s.io images import "spacegate-admin-${VERSION}.tar"
ctr -n k8s.io images import "ai-gateway-service-${VERSION}.tar"
```

## 9. 构建产物对照

| 服务 | 镜像变量 | 主要端口 | 说明 |
| --- | --- | --- | --- |
| SpaceGate 网关 | `SPACEGATE_IMAGE` | `80` / `443` / `9993` | K8s 网关进程，启动参数 `-c k8s:spacegate` |
| SpaceGate Admin | `SPACEGATE_ADMIN_IMAGE` | `9080` | 前端静态资源 + Admin Server，`/api` 代理到容器内 `9081` |
| AI Gateway Service | `AI_GATEWAY_SERVICE_IMAGE` | `18080` | 排队、限流、wait、worker、回调服务 |
| HAI 静态插件 | `SPACEGATE_IMAGE` | 无 | 静态链接到网关二进制；更新 HAI 代码必须替换整个网关镜像 |

## 10. 更新 K8s 镜像参考

执行目录：目标服务器任意目录，前提是 `kubectl` 已连接目标集群。

```bash
export VERSION="<version>"
export SPACEGATE_IMAGE="spacegate:$VERSION"
export SPACEGATE_ADMIN_IMAGE="spacegate-admin:$VERSION"
export AI_GATEWAY_SERVICE_IMAGE="ai-gateway-service:$VERSION"

kubectl set image daemonset/spacegate \
  spacegate="$SPACEGATE_IMAGE" \
  -n spacegate

kubectl set image deployment/spacegate-admin \
  spacegate-admin="$SPACEGATE_ADMIN_IMAGE" \
  -n spacegate

kubectl set image deployment/ai-gateway-service \
  ai-gateway-service="$AI_GATEWAY_SERVICE_IMAGE" \
  -n spacegate

kubectl rollout status daemonset/spacegate -n spacegate --timeout=300s
kubectl rollout status deployment/spacegate-admin -n spacegate --timeout=300s
kubectl rollout status deployment/ai-gateway-service -n spacegate --timeout=300s
```

如果目标集群节点不能从 registry 拉取镜像，需要确保每个会调度 Pod 的节点都已经执行过 `docker load` 或 `ctr images import`。

## 11. 可选：推送到镜像仓库

如果目标环境可以访问镜像仓库，可以在构建机额外执行：

```bash
cd "$WORKSPACE_ROOT"

export REGISTRY="<registry.example.com/spacegate>"

docker tag "$SPACEGATE_IMAGE" "$REGISTRY/spacegate:$VERSION"
docker tag "$SPACEGATE_ADMIN_IMAGE" "$REGISTRY/spacegate-admin:$VERSION"
docker tag "$AI_GATEWAY_SERVICE_IMAGE" "$REGISTRY/ai-gateway-service:$VERSION"

docker push "$REGISTRY/spacegate:$VERSION"
docker push "$REGISTRY/spacegate-admin:$VERSION"
docker push "$REGISTRY/ai-gateway-service:$VERSION"
```

## 12. 自动化构建脚本状态

`spacegate/deploy/k8s/build-offline-service-images.sh` 当前不存在，因此不要引用或执行它。请使用第 2 节的逐步构建流程；该流程会从 `HAI_HUB_ROOT` 构建静态 HAI 网关、保存三个服务镜像，并生成校验文件。
