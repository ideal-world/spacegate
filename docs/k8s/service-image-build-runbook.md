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
cd /Users/yiye/projectSpace/huayun_project/spacegate-workspace

export WORKSPACE_ROOT="$PWD"
export SPACEGATE_ROOT="$WORKSPACE_ROOT/spacegate"
export ADMIN_FE_ROOT="$WORKSPACE_ROOT/spacegate-admin-fe"
export ADMIN_DOCKER_CONTEXT=""
export ADMIN_DOCKER_DIR="/resource/docker/spacegate-admin"
export ARTIFACT_DIR="$WORKSPACE_ROOT/image-artifacts"

# VERSION 建议使用 Git tag、日期版本或交付版本号，例如 20260709-001。
export VERSION="1.0.1"

export SPACEGATE_IMAGE="spacegate:$VERSION"
export SPACEGATE_ADMIN_IMAGE="spacegate-admin:$VERSION"
export AI_GATEWAY_SERVICE_IMAGE="ai-gateway-service:$VERSION"

# Admin 合并镜像的 Nginx 基础镜像。若默认镜像源不可用，可改成本地已导入的 nginx Debian 镜像。
export ADMIN_NGINX_IMAGE="nginx:1.27-bookworm"

# 如果网关镜像需要内置 HAI native dylib，设置到 hai-hub 仓库路径。
export HAI_HUB_ROOT="/Users/yiye/projectSpace/huayun_project/hai-hub"

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

### 2.2 构建 SpaceGate 网关镜像

```bash
cd "$WORKSPACE_ROOT"

docker build --progress=plain \
  --build-context "hai_hub=$HAI_HUB_ROOT" \
  -f "$SPACEGATE_ROOT/resource/docker/spacegate-k8s/Dockerfile" \
  -t "$SPACEGATE_IMAGE" \
  "$SPACEGATE_ROOT"
```

如果这里失败，重点看 `cargo build --manifest-path /hai-hub/Cargo.toml --release -p hai-hub-spacegate-plugins` 上方的 Rust 编译错误；Docker 最后一行 `exit code: 101` 只是汇总错误。

### 2.3 从 SpaceGate 镜像中复制 native 插件制品

```bash
cd "$WORKSPACE_ROOT"
mkdir -p "$ARTIFACT_DIR/native-plugins"

plugin_container_id="$(docker create "$SPACEGATE_IMAGE")"
docker cp "$plugin_container_id:/lib/spacegate/plugins/hai_hub_spacegate_plugins.so" \
  "$ARTIFACT_DIR/native-plugins/hai_hub_spacegate_plugins.so"
docker rm -f "$plugin_container_id"

test -f "$ARTIFACT_DIR/native-plugins/hai_hub_spacegate_plugins.so"
ls -lh "$ARTIFACT_DIR/native-plugins/hai_hub_spacegate_plugins.so"
```

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

shasum -a 256 "$ARTIFACT_DIR"/*.tar \
  "$ARTIFACT_DIR/native-plugins"/*.so \
  > "$ARTIFACT_DIR/SHA256SUMS"

ls -lh "$ARTIFACT_DIR"
cat "$ARTIFACT_DIR/SHA256SUMS"
```

最终把整个目录复制到 U 盘：

```text
spacegate-workspace/image-artifacts/
  spacegate-<version>.tar
  spacegate-admin-<version>.tar
  ai-gateway-service-<version>.tar
  native-plugins/hai_hub_spacegate_plugins.so
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

### 3.1 `hai-hub-spacegate-plugins` 构建和 copy 逻辑

`hai-hub-spacegate-plugins` 不在 `spacegate` 仓库内，它由 `HAI_HUB_ROOT` 指向的 `hai-hub` 仓库提供。它是一个聚合 native dylib crate：Rust 编译原始产物是 `libhai_hub_spacegate_plugins.so`，但镜像内和离线交付时统一重命名为 `hai_hub_spacegate_plugins.so`。这个 `.so` 的 `register(repo)` 入口会一次性注册多个插件 code。

当前聚合 `.so` 注册的插件包括：

| 插件 code | 插件类型 | 作用 |
| --- | --- | --- |
| `hub-request-id` | `RequestIdPlugin` | 为请求注入或透传请求 ID |
| `auth` | `AuthPlugin` | 通用鉴权插件 |
| `hai-observe` | `HaiObservePlugin` | HAI 调用审计、指标与链路字段采集 |
| `hai-auth` | `HaiAuthPlugin` | 基于 HAI API Key 和资产订阅鉴权 |
| `hai-asset` | `HaiAssetPlugin` | 从 Redis 加载并校验 HAI 资产 |
| `hai-quota` | `HaiQuotaPlugin` | 基于资产配置做 QPS/并发限流 |
| `hai-dispatch` | `HaiDispatchPlugin` | 根据资产运行时配置派发上游 |

当前 `hai-hub-spacegate-plugins/Cargo.toml` 的 `[lib] crate-type` 可能只有 `rlib`。因此 Dockerfile 会在 `/hai-hub` 临时副本中把 crate type 改为同时包含 `dylib`，再执行普通 `cargo build` 产出 Linux `.so`。

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
RUN sed -i '/"backend\/hai-hub-resource"/d;/"services\/hai-hub-all"/d;/"services\/hai-hub-spacegate"/d;/"services\/hai-hub-auth-plugin"/d' /hai-hub/Cargo.toml
RUN sed -i 's/crate-type = \[ "rlib" \]/crate-type = [ "rlib", "dylib" ]/' /hai-hub/backend/hai-hub-spacegate-plugins/Cargo.toml \
    && grep -n 'crate-type = .*dylib' /hai-hub/backend/hai-hub-spacegate-plugins/Cargo.toml
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
RUN CARGO_PROFILE_RELEASE_LTO=false CARGO_PROFILE_RELEASE_PANIC=unwind \
    cargo build --manifest-path /hai-hub/Cargo.toml --release -p hai-hub-spacegate-plugins \
    && test -f /hai-hub/target/release/libhai_hub_spacegate_plugins.so
```

这里的 `sed` 只修改 Docker build 中的 `/hai-hub` 临时副本，不会修改本地 `HAI_HUB_ROOT`。原因是 `hai-hub` workspace 还有其他服务 member，这些 member 可能依赖构建上下文之外的兄弟仓库；但构建 SpaceGate native 插件只需要 `backend/hai-hub-spacegate-plugins`。

如果是在 Linux 构建机上直接构建 `hai-hub-spacegate-plugins`，可以执行以下命令。注意：macOS 本机构建会生成 `.dylib`，不能作为 Linux K8s 部署使用；Linux `.so` 推荐由上面的 Dockerfile 或 Linux 构建机生成。如果 Linux 构建机缺少 `hai-hub` 其他 workspace member 依赖的兄弟仓库，也需要先在临时副本中裁剪 workspace，只保留 `backend/hai-hub-spacegate-plugins`。

```bash
cd "$WORKSPACE_ROOT"
mkdir -p "$HAI_HUB_ROOT/.cargo"

if [[ -f "$HAI_HUB_ROOT/.cargo/config.toml" ]]; then
  cp "$HAI_HUB_ROOT/.cargo/config.toml" \
    "$HAI_HUB_ROOT/.cargo/config.toml.bak.$(date +%Y%m%d%H%M%S)"
fi

cp "$HAI_HUB_ROOT/backend/hai-hub-spacegate-plugins/Cargo.toml" \
  "$HAI_HUB_ROOT/backend/hai-hub-spacegate-plugins/Cargo.toml.bak.$(date +%Y%m%d%H%M%S)"

cat > "$HAI_HUB_ROOT/.cargo/config.toml" <<EOF
[patch."https://github.com/ideal-world/spacegate"]
spacegate-config = { path = "$SPACEGATE_ROOT/crates/config" }
spacegate-ext-axum = { path = "$SPACEGATE_ROOT/crates/extension/axum" }
spacegate-ext-redis = { path = "$SPACEGATE_ROOT/crates/extension/redis" }
spacegate-kernel = { path = "$SPACEGATE_ROOT/crates/kernel" }
spacegate-model = { path = "$SPACEGATE_ROOT/crates/model" }
spacegate-plugin = { path = "$SPACEGATE_ROOT/crates/plugin" }
spacegate-shell = { path = "$SPACEGATE_ROOT/crates/shell" }
EOF

sed -i 's/crate-type = \[ "rlib" \]/crate-type = [ "rlib", "dylib" ]/' \
  "$HAI_HUB_ROOT/backend/hai-hub-spacegate-plugins/Cargo.toml"

cargo build \
  --manifest-path "$HAI_HUB_ROOT/Cargo.toml" \
  --release \
  -p hai-hub-spacegate-plugins

test -f "$HAI_HUB_ROOT/target/release/libhai_hub_spacegate_plugins.so"

mkdir -p "$ARTIFACT_DIR/native-plugins"
cp "$HAI_HUB_ROOT/target/release/libhai_hub_spacegate_plugins.so" \
  "$ARTIFACT_DIR/native-plugins/hai_hub_spacegate_plugins.so"
```

如果采用 K8s volume 挂载 native 插件，可以把上面的 `.so` 放到目标节点或镜像制品目录，并在 Pod 中挂载到：

```text
/var/lib/spacegate/plugins/hai_hub_spacegate_plugins.so
```

然后 Dockerfile 将 `.so` 复制进最终 SpaceGate 镜像：

```dockerfile
COPY --from=hai-plugin-builder \
  /hai-hub/target/release/libhai_hub_spacegate_plugins.so \
  /lib/spacegate/plugins/
```

最终镜像内置插件路径：

```text
/lib/spacegate/plugins/hai_hub_spacegate_plugins.so
```

一键脚本还会把镜像内的 `.so` 复制到离线制品目录：

```bash
docker create "$SPACEGATE_IMAGE"
docker cp "<container-id>:/lib/spacegate/plugins/hai_hub_spacegate_plugins.so" \
  "$ARTIFACT_DIR/native-plugins/hai_hub_spacegate_plugins.so"
```

这个单独 `.so` 文件不是运行网关镜像的必要条件；它用于部署前核对、归档，或在需要 K8s volume 方式挂载 native 插件时使用。

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

快速检查镜像内是否包含二进制和内置 dylib：

```bash
docker run --rm --entrypoint sh "$SPACEGATE_IMAGE" -c '
  set -e
  test -x /usr/local/bin/spacegate
  ls -l /lib/spacegate/plugins/*.so
'
```

K8s 运行时建议保留以下环境变量，使网关同时扫描镜像内置插件和 K8s volume 挂载插件：

```yaml
- name: PLUGINS
  value: /lib/spacegate/plugins,/var/lib/spacegate/plugins
```

如果需要额外挂载 `.so`，建议挂载到 `/var/lib/spacegate/plugins`，不要直接覆盖 `/lib/spacegate/plugins`，否则会遮蔽镜像内置插件。

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

mkdir -p "$ARTIFACT_DIR/native-plugins"
plugin_container_id="$(docker create "$SPACEGATE_IMAGE")"
docker cp "$plugin_container_id:/lib/spacegate/plugins/hai_hub_spacegate_plugins.so" \
  "$ARTIFACT_DIR/native-plugins/hai_hub_spacegate_plugins.so"
docker rm -f "$plugin_container_id"

shasum -a 256 "$ARTIFACT_DIR"/*.tar \
  "$ARTIFACT_DIR/native-plugins"/*.so \
  > "$ARTIFACT_DIR/SHA256SUMS"

ls -lh "$ARTIFACT_DIR"
cat "$ARTIFACT_DIR/SHA256SUMS"
```

离线交付目录：

```text
spacegate-workspace/image-artifacts/
  spacegate-<version>.tar
  spacegate-admin-<version>.tar
  ai-gateway-service-<version>.tar
  native-plugins/hai_hub_spacegate_plugins.so
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
| HAI native 插件 | `native-plugins/hai_hub_spacegate_plugins.so` | 无 | 已内置在 SpaceGate 镜像中；也可用于 K8s volume 挂载 |

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

## 12. 可选：一键构建全部镜像和离线制品

执行目录：`spacegate-workspace`

执行脚本：`spacegate/deploy/k8s/build-offline-service-images.sh`

当前建议先用第 2 节逐步命令跑通，再使用本脚本。该脚本会完成以下动作：

1. 构建 `spacegate` 网关镜像，并在镜像内构建和内置 `hai-hub-spacegate-plugins` dylib。
2. 从网关镜像中复制 `hai_hub_spacegate_plugins.so` 到离线制品目录，方便单独核对或挂载。
3. 构建 `spacegate-admin-server` 后端二进制。
4. 构建 `spacegate/sdk/admin-client` 和 `spacegate-admin-fe` 前端静态资源。
5. 组装 `spacegate-admin` 前后端合并镜像。
6. 构建 `ai-gateway-service` 镜像。
7. 将三个镜像保存为 tar 包，并生成 SHA-256 校验文件。

执行：

```bash
cd "$WORKSPACE_ROOT"

VERSION="$VERSION" \
HAI_HUB_ROOT="$HAI_HUB_ROOT" \
bash "$SPACEGATE_ROOT/deploy/k8s/build-offline-service-images.sh"
```

脚本输出的离线制品目录：

```text
spacegate-workspace/image-artifacts/
  spacegate-<version>.tar
  spacegate-admin-<version>.tar
  ai-gateway-service-<version>.tar
  native-plugins/hai_hub_spacegate_plugins.so
  SHA256SUMS
  IMAGES.txt
```
