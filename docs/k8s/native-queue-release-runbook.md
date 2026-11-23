# SpaceGate 与新 Admin 镜像交付及 K8s 发布操作手册

维护日期：2026-11-20。适用当前独立 HAI 网关、新 Web + Node API + Rust 管理服务，以及原生 `hai-quota` 队列。

本文替代旧 `service-image-build-runbook.md` 中的新版本发布主流程。旧文档保留作旧 Vue / Wasm 服务交付参考。本文是已有 K8s 环境的升级手册，不是空集群的一键安装脚本。

**给运维的部署文件包：** [native-queue-release/README.md](../../resource/kube-manifests/native-queue-release/README.md)。配置已简化为每个服务一份 `.env`：`spacegate-admin.env` 与 `spacegate.env`，普通项和凭据写在一起，整份导入并由对应工作负载加载。模板见 [配置说明](../../../spacegate-admin/deploy/k8s/README.md)，镜像导出和交付步骤见 [工作区交付手册](../../../镜像导出与配置交付.md)。旧版集中 YAML 和拆分配置不再作为发布入口。

## 1. 先选择本次操作范围

| 场景 | 操作章节 |
| --- | --- |
| 导出本地已经验收的镜像，去测试环境发布 | 2 → 3 → 5～9 → 10 |
| 源码又发生变化，需要重新构建 | 2 → 4 → 3.3 → 5～9 → 10 |
| 首次从旧 Vue Admin 切换 | 必须执行 6、7；不能只更新 image |
| 已完成新 Admin 切换，日常仅更新代码镜像 | 保留已有密钥和数据，按 7.5 更新镜像 |
| 排查或回滚 | 11、12 |

每个代码块都标注执行位置。不要跨机器依赖之前 shell 的变量；在目标服务器重新执行对应变量块。遇到命令失败，应停止该发布步骤，排查后再继续。

### 1.1 本次实际交付什么

| 交付物 | 职责 | 默认是否交付 |
| --- | --- | --- |
| `spacegate:<version>` | 网关转发、HAI 静态插件、原生队列 | 网关或插件有更新时交付 |
| `spacegate-admin:<version>` | 新 Web、Node API、Rust 配置管理服务 | 后台有更新时交付 |
| `ai-gateway-service` | 旧 Wasm 队列后端 | 只有实际旧绑定仍依赖时保留；原生队列不要求更新它 |
| 外部 `.wasm`、第三方 `.so` | 实际配置引用的扩展插件 | 按配置单独核对、分发 |

新 Admin 的链路为：

```text
浏览器 → Nginx :9080 → 新 Web
                    → /api/v1/* → Node :3000 → Rust :9081 → K8s 配置 / 网关
                                           → 网关内部管理 API → 原生队列
```

`3000`、`9081` 在合并后台内监听 loopback，不需要建立公网 Service。模型请求仍通过网关业务监听器，不通过 Admin。`hai-quota` 已静态编译进网关，无单独 HAI `.so` 需要复制。

### 1.2 制品边界

`docker save` 保存镜像及镜像默认配置，不保存运行容器注入的环境变量、容器可写层、bind mount、named volume 或外部数据库/Redis/S3 数据。不要用 `docker export` 或 `docker commit` 代替本手册的镜像交付。

本文本地配置对照来自 2026-11-20 的容器环境和挂载配置；后续发布要重新记录镜像 ID。文档中的本地 ID 和测试值不是永久的版本锁定。

### 1.3 执行工具与权限

| 位置 | 所需工具/前提 |
| --- | --- |
| 本机构建/导出 | Docker、Bash、Git、SHA-256 工具；仅从源码构建才需源码目录和可用依赖源 |
| 目标运维机 | kubectl、jq、rg、curl、Bash；节点镜像导入使用实际容器运行时 CLI |
| 首次密钥与接口请求文件准备 | Node.js 22/24（支持 OpenSSL SM2）及 openssl；可在安全本机生成后私下分发，不要求在每个 K8s 节点安装 Node |
| 集群 | 现有 CRD、ServiceAccount/RBAC、可用数据库/Redis/对象存储，以及明确的测试发布窗口 |

使用源码脚本的机器要显式设置该机器上的 `ADMIN_ROOT`；不要把 macOS 路径当作 Linux 服务器路径。私有密钥跨机器分发使用既有安全通道，与镜像 tar 分开管理。

## 2. 构建机设置变量

执行位置：macOS 本机 `spacegate-workspace`。本节变量在同一 shell 中供第 3、4 节使用。

```bash
cd /Users/yiye/projectSpace/huayun_project/spacegate-workspace

export WORKSPACE_ROOT="$PWD"
export SPACEGATE_ROOT="$WORKSPACE_ROOT/spacegate"
export ADMIN_ROOT="$WORKSPACE_ROOT/spacegate-admin"
export HAI_HUB_ROOT="/Users/yiye/projectSpace/huayun_project/hai-workspace/hai-hub"

# 每次正式交付使用新版本，不复用已经交付的 tag。
export VERSION="v2026.11.20-queue-001"
export PLATFORM="linux/amd64"
export SPACEGATE_IMAGE="spacegate:$VERSION"
export ADMIN_IMAGE="spacegate-admin:$VERSION"
export ARTIFACT_DIR="$WORKSPACE_ROOT/image-artifacts/$VERSION"

mkdir -p "$ARTIFACT_DIR"
test -f "$ADMIN_ROOT/images/combined.Dockerfile"
test -f "$SPACEGATE_ROOT/resource/docker/spacegate-runtime/hai.Dockerfile"
```

目标节点为 x86_64 时使用 `linux/amd64`；ARM64 节点需对应的 `linux/arm64` 镜像。重打标签不能改变架构。日期自动生成可使用 `VERSION="v$(date +%Y.%m.%d)-001"`，同日重复交付仍须递增序号。

## 3. 推荐：导出本地正在运行、已验收的镜像

本流程不重新编译，不停止本地容器，也不复制本地业务数据。

### 3.1 锁定容器使用的镜像 ID

执行位置：构建机。

```bash
export SOURCE_GATEWAY_CONTAINER="hai-hub-test-spacegate"
export SOURCE_ADMIN_CONTAINER="hai-hub-test-spacegate-admin"

export SOURCE_GATEWAY_ID="$(docker inspect --format '{{.Image}}' "$SOURCE_GATEWAY_CONTAINER")"
export SOURCE_ADMIN_ID="$(docker inspect --format '{{.Image}}' "$SOURCE_ADMIN_CONTAINER")"

docker image inspect "$SOURCE_GATEWAY_ID" "$SOURCE_ADMIN_ID" \
  --format '{{.Id}} {{.Os}}/{{.Architecture}} {{.Created}}'

docker image inspect "$SOURCE_GATEWAY_ID" "$SOURCE_ADMIN_ID" \
  --format '{{.Id}} {{.Os}}/{{.Architecture}} {{.Created}}' \
  > "$ARTIFACT_DIR/image-provenance.txt"

docker inspect "$SOURCE_GATEWAY_CONTAINER" "$SOURCE_ADMIN_CONTAINER" \
  --format '{{.Name}}{{range .Mounts}} mount={{.Destination}}{{end}}' \
  > "$ARTIFACT_DIR/runtime-mount-paths.txt"
```

核对架构与第 2 节一致，确认这些容器就是本次验收对象。使用容器的不可变 image ID 打标签，避免 `hub-test` 标签后来变化而导出错版。

不要把完整 `docker inspect` 或私有环境文件放入交付目录：它们可能含密码。若容器启动后手工改过二进制/静态资源，改动不属于镜像，先确认修改已进入正式构建产物再导出。

### 3.2 打交付标签

```bash
docker tag "$SOURCE_GATEWAY_ID" "$SPACEGATE_IMAGE"
docker tag "$SOURCE_ADMIN_ID" "$ADMIN_IMAGE"

docker image inspect "$SPACEGATE_IMAGE" "$ADMIN_IMAGE" \
  --format '{{.RepoTags}} {{.Id}} {{.Os}}/{{.Architecture}}'
```

这一步不会改变镜像内容。若只发布 Admin，仅执行 Admin 对应命令，后续也只导出、导入和更新 Admin。

### 3.3 导出 tar 与校验文件

```bash
docker save -o "$ARTIFACT_DIR/spacegate-${VERSION}.tar" "$SPACEGATE_IMAGE"
docker save -o "$ARTIFACT_DIR/spacegate-admin-${VERSION}.tar" "$ADMIN_IMAGE"

cd "$ARTIFACT_DIR"
shasum -a 256 *.tar > SHA256SUMS
shasum -a 256 -c SHA256SUMS
```

在制品目录内生成校验文件，因此保存的是相对文件名，复制到 Linux 后仍可校验。当前目录只存本次版本的制品，不混入私有配置。

交付目录示例：

```text
image-artifacts/v2026.11.20-queue-001/
  spacegate-v2026.11.20-queue-001.tar
  spacegate-admin-v2026.11.20-queue-001.tar
  SHA256SUMS
  image-provenance.txt
  runtime-mount-paths.txt
```

只更新一个服务时不必导出另一个。镜像 ID 是构建制品身份；tar 的 SHA-256 是文件传输校验，两者用途不同。

## 4. 备用：从当前源码构建

只有代码未进入已验收镜像，或目标架构不同等情况才使用本节。构建完成后需要验收新制品，不能沿用旧镜像的验收结论。

### 4.1 记录构建来源

执行位置：构建机；先执行第 2 节。

```bash
git -C "$SPACEGATE_ROOT" rev-parse HEAD > "$ARTIFACT_DIR/spacegate-head.txt"
git -C "$SPACEGATE_ROOT" status --short > "$ARTIFACT_DIR/spacegate-status.txt"
git -C "$ADMIN_ROOT" rev-parse HEAD > "$ARTIFACT_DIR/admin-head.txt"
git -C "$ADMIN_ROOT" status --short > "$ARTIFACT_DIR/admin-status.txt"
git -C "$HAI_HUB_ROOT" rev-parse HEAD > "$ARTIFACT_DIR/hai-hub-head.txt"
git -C "$HAI_HUB_ROOT" status --short > "$ARTIFACT_DIR/hai-hub-status.txt"
```

Docker 使用当前工作树，包括未提交文件；只有 HEAD 不能完整描述这样的构建来源。构建期间不要同时更新源码。需要长期重建能力时，另行保存本次授权发布的源码快照及依赖锁文件；上述状态清单不是源码备份。

### 4.2 构建纯 HAI 网关

```bash
docker build --platform "$PLATFORM" --progress=plain \
  --build-context "hai_hub=$HAI_HUB_ROOT" \
  --build-arg CARGO_BUILD_JOBS=2 \
  -f "$SPACEGATE_ROOT/resource/docker/spacegate-runtime/hai.Dockerfile" \
  -t "$SPACEGATE_IMAGE" "$SPACEGATE_ROOT"
```

此入口支持 K8s，但不内置旧队列 Wasm 文件，也不设置旧完整 K8s 镜像的 `K8S_NAMESPACE`、`PLUGIN_DIR` 默认值。第 7、8 节必须落实。

若明确仍需旧内置 Wasm 制品，可改用完整入口，二选一：

```bash
docker build --platform "$PLATFORM" --progress=plain \
  --build-context "hai_hub=$HAI_HUB_ROOT" \
  -f "$SPACEGATE_ROOT/resource/docker/spacegate-k8s/Dockerfile" \
  -t "$SPACEGATE_IMAGE" "$SPACEGATE_ROOT"
```

完整入口会多编译旧 Wasm，耗时更长；其 namespace/目录默认值仍须按目标环境覆盖。基础镜像、Cargo registry 不可达时停在具体失败步骤处理，不改依赖版本掩盖网络错误。

### 4.3 构建新合并 Admin

```bash
docker build --platform "$PLATFORM" --progress=plain \
  --build-context "spacegate_src=$SPACEGATE_ROOT" \
  --build-arg CARGO_BUILD_JOBS=2 \
  -f "$ADMIN_ROOT/images/combined.Dockerfile" \
  -t "$ADMIN_IMAGE" "$ADMIN_ROOT"
```

不需要执行旧 SDK/Vue 的 `npm build` 或 `rsync dist`。新镜像在 builder 内构建新 Web、Node 和迁入 Admin 仓库的 Rust 服务。

下载受限时可增加 `--build-arg NPM_REGISTRY=https://registry.npmmirror.com`；`SQLITE_PREBUILD_MIRROR` 仅用于实际 SQLite 制品下载问题，并填写已核验的下载源。基础镜像支持 `NODE_IMAGE`、`RUST_IMAGE` 构建参数；替换前确认架构和运行库兼容。

### 4.4 检查镜像文件

```bash
docker run --rm --entrypoint sh "$SPACEGATE_IMAGE" -c \
  'test -x /usr/local/bin/spacegate'

docker run --rm --entrypoint sh "$ADMIN_IMAGE" -c \
  'test -x /usr/local/bin/spacegate-admin-server && test -f /app/dist/index.js && test -f /usr/share/nginx/html/index.html && nginx -t'

docker image inspect "$SPACEGATE_IMAGE" "$ADMIN_IMAGE" \
  --format '{{.RepoTags}} {{.Id}} {{.Os}}/{{.Architecture}}' \
  > "$ARTIFACT_DIR/image-provenance.txt"
```

这是制品结构检查，不代表登录、K8s 连接和模型链路通过。HAI 网关入口不是旧 CLI，不能靠追加旧 `-c` 参数或 `--help` 作为通用烟测。导出按第 3.3 节执行。

## 5. 分发镜像并核对测试集群

### 5.1 目标服务器变量

在拥有目标集群 kubeconfig 的运维机重新设置：

```bash
export VERSION="v2026.11.20-queue-001"
export NS="spacegate"
export GATEWAY_NS="$NS"
export ADMIN_DEPLOYMENT="spacegate-admin"
export ADMIN_CONTAINER="spacegate-admin"
export GATEWAY_DAEMONSET="spacegate"
export GATEWAY_CONTAINER="spacegate"
export SPACEGATE_IMAGE="spacegate:$VERSION"
export ADMIN_IMAGE="spacegate-admin:$VERSION"
export DELIVERY_DIR="/实际复制到服务器的制品目录"

kubectl config current-context
kubectl get nodes -o custom-columns=NAME:.metadata.name,ARCH:.status.nodeInfo.architecture,RUNTIME:.status.nodeInfo.containerRuntimeVersion
kubectl -n "$NS" get deployment "$ADMIN_DEPLOYMENT"
kubectl -n "$GATEWAY_NS" get daemonset "$GATEWAY_DAEMONSET"
```

确认是目标测试集群，不能沿用未知 context。这里按现有 DaemonSet 网关拓扑编写；若实际网关是 Deployment，替换资源种类并核对实例发现机制，不能直接照抄 DaemonSet 命令。

### 5.2 离线导入

将完整制品目录复制到每个可能运行对应 Pod 的节点。节点实际使用 containerd 时：

```bash
cd "$DELIVERY_DIR"
sha256sum -c SHA256SUMS

sudo ctr -n k8s.io images import "spacegate-${VERSION}.tar"
sudo ctr -n k8s.io images import "spacegate-admin-${VERSION}.tar"
sudo ctr -n k8s.io images ls
```

节点实际使用 Docker/cri-dockerd 时才使用：

```bash
docker load -i "$DELIVERY_DIR/spacegate-${VERSION}.tar"
docker load -i "$DELIVERY_DIR/spacegate-admin-${VERSION}.tar"
```

K3s/RKE2 等环境应使用其实际 containerd socket/配套 CLI，不能向另一套 containerd 导入。清单引用必须匹配运行时导入后的完整镜像名；如果显示为 `docker.io/library/spacegate:<version>`，使用该等价完整引用核对。离线使用 `IfNotPresent`，不要使用强制远端拉取的 `Always`。

### 5.3 有仓库时的替代路径

在构建机执行，仓库地址换成实际值：

```bash
export REGISTRY="image.hyit.com.cn/ai-hai"
docker tag "$SPACEGATE_IMAGE" "$REGISTRY/spacegate:$VERSION"
docker tag "$ADMIN_IMAGE" "$REGISTRY/spacegate-admin:$VERSION"
docker push "$REGISTRY/spacegate:$VERSION"
docker push "$REGISTRY/spacegate-admin:$VERSION"
```

目标运维机相应设置 `SPACEGATE_IMAGE` 和 `ADMIN_IMAGE` 为完整仓库名，保留现有 `imagePullSecrets`。不要同一批节点混用不同内容的同名 tag。

## 6. 配置变量、当前本地实例与首次准备

### 6.1 当前本地运行参照（2026-11-20）

| 服务 | 容器 | 本地标签 | 本次核对的镜像 ID 前缀 / 架构 |
| --- | --- | --- | --- |
| Gateway | `hai-hub-test-spacegate` | `spacegate:hub-test` | `db78144c7705` / `linux/amd64` |
| Admin | `hai-hub-test-spacegate-admin` | `spacegate-admin:hub-test` | `13cca4ecb658` / `linux/amd64` |

以下是容器启动环境，而不是仅阅读 `.env.example` 推断。未注入的值标注为默认/未注入；入口脚本会在子进程启动前固定内部地址。所有凭据均脱敏，不可把 `<REDACTED>` 当成可运行的值。

### 6.2 Admin 参数对照

| 变量 | 当前本地值 / 格式 | 测试环境处理与常见错误 |
| --- | --- | --- |
| `NODE_ENV` | `development` | 受控测试可显式设 `test`；生产设 `production`。镜像默认 production，不能只改 HAI_CORE_ENV 就认为关闭生产守卫 |
| `HAI_CORE_ENV` | `development` | 与 NODE_ENV 配对；测试 `test`，生产 `production` |
| `HAI_CORE_LOGGING_LEVEL` | `info` | 可保留；不要为排障把完整私有配置输出日志 |
| `CONFIG` | `file:/etc/spacegate` | K8s 改为 `k8s:<namespace>`；不要复制本地文件模式 |
| `GATEWAY_API_URL` | `http://spacegate:8888` | 文件模式的独立网关地址；K8s 实例目录使用发现到的节点地址，不靠它替换 Pod discovery |
| `GATEWAY_CLASS_NAME` | 本地文件模式未注入 | K8s 必须匹配现有 GatewayClass |
| `GATEWAY_INSTANCE` | 本地文件模式未注入 | K8s 为 `<DaemonSet名称>.<namespace>`，例如 `spacegate.spacegate` |
| `HOST` / `PORT` | `127.0.0.1` / `3000` | 合并入口固定；不要改成对外 9080，外层 Nginx 监听 9080 |
| `HAI_SERV_CORS_ORIGIN` | `http://127.0.0.1:9080,http://localhost:9080` | 改实际后台 Origin；不带业务路径。生产必须显式设置，推荐 HTTPS |
| `PUBLIC_API_BASE` | 未注入，镜像模板按空值处理 | 留空使用同源。不能填内部 DNS，也不要填末尾 `/api/v1` |
| `HAI_ADMIN_DEFAULT_PASSWORD` | `<REDACTED>` | 空用户库首次创建 admin；旧库已有用户时不重设密码 |
| `HAI_CRYPTO_TRANSPORT_SERVERKEYRING` | 单行 JSON，见 6.5 | 全部副本共享同一密钥环；不得每次发布重新生成 |
| `HAI_SPACEGATE_ADMINAK` | `<REDACTED>` | 内部服务身份，可设 `spacegate-admin`；不是用户登录名 |
| `HAI_SPACEGATE_ADMINSK` | `<REDACTED>` | 内部 Rust/Node 配对密钥；不要只设置旧 `SK` |
| `HAI_RUST_JWT_KEY` | `<REDACTED>` | 随机 32 字节的 Base64；不是普通 UUID/任意中文字符串 |
| `HAI_SPACEGATE_ADMINBASEURL` | 入口固定 `http://127.0.0.1:9081` | 不填网关地址；无须在 K8s 暴露 9081 |
| `HAI_SPACEGATE_AIGATEWAYBASEURL` | `http://ai-gateway-service:18080` | 旧服务入口；本地地址存在不表示原生队列依赖它。不使用旧功能时不需要部署该服务 |
| `HAI_SPACEGATE_NATIVEQUEUETARGETS` | 单行 JSON，见 6.5 | baseUrl 是网关内部管理地址，不是 Rust/旧队列地址 |
| `HAI_QUEUE_ADMIN_TOKEN` | `<REDACTED>` | 与网关一致，至少 32 字符；不写入前端或插件 JSON |
| `HAI_DB_TYPE` | `postgresql` | 推荐保留，连接单独 Admin 数据库 |
| `HAI_DB_HOST` / `HAI_DB_PORT` | `host.docker.internal` / `5432` | 改实际 PG Service/DNS；K8s 不使用本机 Docker 宿主别名 |
| `HAI_DB_DATABASE` / `HAI_DB_USER` | `spacegate_admin` / `user` | 数据库名与有权限账号；不要指向 Hub 业务库当作账号迁移 |
| `HAI_DB_PASSWORD` | `<REDACTED>` | 填写在 Admin 同一份配置文件，部署时整体导入 |
| `HAI_DB_SSL` / `HAI_DB_URL` | 本地未注入，YAML 回退 `false` / 空 | 按实际 TLS 配置；非空 URL 优先，残留 URL 会覆盖分项连接字段 |
| `HAI_CACHE_TYPE` | `redis` | 保留共享 Redis；Admin Redis 与队列 Redis 是独立用途 |
| `HAI_CACHE_HOST` / `HAI_CACHE_PORT` | `host.docker.internal` / `6379` | 改集群实际连接地址 |
| `HAI_CACHE_DB` / `HAI_CACHE_KEYPREFIX` | `0` / `spacegate-admin_` | 保留隔离前缀；改前缀会影响已有会话可见性 |
| `HAI_CACHE_PASSWORD` / `HAI_CACHE_URL` | 本地未注入 | 按目标 Redis 注入；不要遗留另一环境 URL |
| `HAI_STORAGE_TYPE` | `s3` | 用于 Admin 头像等资源，不是网关请求体 |
| `HAI_STORAGE_ENDPOINT` | `http://host.docker.internal:19000` | 改目标 S3 endpoint；不能把 localhost 当成远端对象存储 |
| `HAI_STORAGE_BUCKET` / `HAI_STORAGE_REGION` | `spacegate-admin-hub-test` / `us-east-1` | 预先准备 bucket，region 按实际服务 |
| `HAI_STORAGE_FORCEPATHSTYLE` | `true` | 本地 SeaweedFS 使用；云存储按实际寻址要求设置 |
| `HAI_STORAGE_ACCESSKEYID` / `HAI_STORAGE_SECRETACCESSKEY` | `<REDACTED>` | Admin 自己的对象存储权限；不能仅配置网关 AWS_* 替代 |
| `HAI_REACH_PROVIDERS` | 未注入，YAML 为 email/sms 的 console Provider | test 模式可作受控测试；production 拒绝 console，必须配置真实 Provider |
| `HAI_SERV_MINIAPP_PORT` | 未注入 | 本手册不发布小程序通道，不设置该变量 |

本地 Admin 还继承了队列私有环境文件中的 `AWS_ACCESS_KEY_ID` / `AWS_SECRET_ACCESS_KEY`。这是本地编排共享 env_file 的结果，不是 Admin 存储模块要求；目标环境将这两项只给需要请求体存储的网关。

### 6.3 网关参数对照

| 变量/配置 | 当前本地值 | K8s 目标值与注意点 |
| --- | --- | --- |
| `STARTUP_MODE` | `file` | `k8s`；不要把本地运行环境一起迁移 |
| `CONFIG_FILE_PATH` | `/etc/spacegate` | K8s 不设置本地文件路径 |
| `K8S_NAMESPACE` | 文件模式未注入 | 管理的实际 namespace |
| `GATEWAY_CLASS_NAME` | 文件模式未注入 | 实际 GatewayClass |
| `PLUGIN_DIR` | `/var/lib/spacegate/plugins` | 与第三方 dylib 目录挂载一致；HAI 入口读取单一 PLUGIN_DIR，不靠旧 PLUGINS 列表替代 |
| `RUST_LOG` | `info,hyper::proto=off` | 可保留或按运维要求设置 |
| `HAI_QUEUE_ADMIN_TOKEN` | `<REDACTED>` | 与 Admin 相同 token；后续不随意轮换 |
| `AWS_ACCESS_KEY_ID` / `AWS_SECRET_ACCESS_KEY` | `<REDACTED>` | 网关请求体 S3/OBS 凭据；临时凭据需要时另注入 AWS_SESSION_TOKEN |
| `api_port` | 本地 config.json 为 `8888` | K8s 当前源码默认 `9876`；以实际监听与发现结果为准，不能照搬旧文档 9993 |
| quota `redis_url` | 未显式设置 | 当前回退 Gateway 级 Redis；不等于“队列不需要 Redis”。目标需核对实际 Gateway Redis/插件私有 Redis |
| Gateway `parameters.redis_url` | `redis://redis:6379/0` | 本地由 Gateway 提供 Redis 连接；测试环境换为实际 Redis。不要误将 Admin 的 HAI_CACHE_HOST 当成此字段的覆盖 |

现有标准 K8s 实例发现按 DaemonSet 的 host IP 与默认管理端口组合地址，适配仓库的 hostNetwork 拓扑。改变为非 hostNetwork、其他端口或 Deployment 前，要验证发现与 reload，不能只调整 queue target 后宣称实例发现也通过。

### 6.4 先准备外部服务

1. 创建独立 Admin 数据库，账号具备目标 schema 的建表、索引及读写权限。IAM、审计、通知、调度器等表由应用初始化，不要求额外手工 SQL；这不代表已有账号会自动迁移。
2. 准备 Admin Redis 与隔离前缀；准备队列 Redis。可以复用实例，但用途和故障影响要明确。
3. 队列 Redis 当前要求单主、加载完成、`noeviction`，不支持 Redis Cluster；账号须能执行队列 Lua/键操作以及 `INFO all`。新网关从 INFO 的 `maxmemory_policy` 读取淘汰策略，不再强制要求 CONFIG 权限；仅兼容服务未返回该字段时才尝试只读的 `CONFIG GET maxmemory-policy`。无法确认策略时保持拒绝准入，不能绕过健康检查上线。
4. 准备 Admin bucket 和网关请求体专用 bucket/prefix。网关需要上传、读取、删除及 multipart 相关权限；启用版本控制时核对版本清理权限。
5. 核对 Collector/ClickHouse 地址与链路，保留原访问审计配置。只发布 Admin/网关不要求重新部署这些服务。

生产模式必须注入真实通知 Provider（SMTP/SMS/API）并匹配 `_reach.yml` 中的模板名称。本文不提供假通知地址以绕过启动校验。无通知设施的受控测试可同时显式设置 `NODE_ENV=test`、`HAI_CORE_ENV=test`；它不等同于完成生产模式验收。

### 6.5 JSON 与 env 文件格式

当前本地队列目标数组：

```json
[{"id":"hub-test","gateway":"hai-hub","runtimeGateway":"hai-hub-dev","baseUrl":"http://spacegate:8888","tokenEnv":"HAI_QUEUE_ADMIN_TOKEN"}]
```

`gateway` 是 Admin 的配置键，`runtimeGateway` 是运行时 `gateway.name`；本地二者不同，不能删除或随意互换。K8s 常为同一名称，但需实查。`id` 是后台选择的运行实例标识，不是 Pod UID。

多进程使用独立的稳定目标地址逐一核验，不用一个负载均衡 Service 的一次成功冒充全部副本验收；共享 Redis 的组计数不重复相加。动态 Pod IP 不适合长期硬编码；hostNetwork 节点地址也要纳入节点变更维护。

当前本地密钥环形状（仅示意，已脱敏）：

```json
{"activeKeyId":"local-v1","keys":{"local-v1":{"publicKey":"<REDACTED_PUBLIC_KEY>","privateKey":"<REDACTED_PRIVATE_KEY>"}}}
```

- 使用 `kubectl --from-env-file` 时每行是 `KEY=value`，不要写 `export`，不要加 shell 外层引号，也不要 `source` 这份文件。JSON 直接作为单行值。
- 使用 YAML 的 `env.value` 时值必须是字符串；JSON 可以使用 YAML 单引号包裹，但引号不属于实际值。
- HAI 配置值会按 YAML 解析。纯数字密码等特殊字面值可能需在实际 HAI 环境值内保留 YAML 字符串引号；自动生成的带字母前缀密码/十六进制随机密钥可减少这种歧义。不要把需要原样比较的 queue token 随意加上实际引号。
- 密钥环 JSON 可以多 key，但 activeKeyId 必须对应 keys 中现存项。SM2 key 必须用生成器生成，不能用占位字符串或普通 UUID。

### 6.6 每个服务一份配置

从 [两份模板](../../../spacegate-admin/deploy/k8s/README.md) 复制并交付：

| 文件 | 内容 |
| --- | --- |
| `config/spacegate-admin.env` | Admin 的全部部署参数，包含数据库/缓存/头像存储、SM2/JWT/内部凭据、队列管理目标和 token |
| `config/spacegate.env` | 网关启动模式、namespace、插件目录、队列管理 token 和请求体存储 AK/SK |

所有活动字段已填格式有效的示例值或有意留空，按 `[需替换]` / `[需核对]` 修改。SM2 示例是真实算法生成的公开示例密钥对，JWT 示例为 32 字节的 Base64；正式环境必须换成自己的密钥。已有部署升级沿用原密钥，不随镜像版本重新生成。两个文件的 `HAI_QUEUE_ADMIN_TOKEN` 必须完全相同。

普通参数与凭据放在同一文件，不再另建 token/key/secrets 文件。生产通知 Provider 也在 Admin 同一文件填写；test 使用镜像默认 console Provider。所有中间件地址、账号、密码、bucket 等必须与目标服务实际一致，模板不会创建第三方账号和桶。

以下从解包交付目录执行：

```bash
export RELEASE_DIR=/收到的交付目录
export VERSION="$(cat "$RELEASE_DIR/VERSION")"
export PRIVATE_DIR="$RELEASE_DIR/generated"
umask 077
mkdir -m 700 "$PRIVATE_DIR"
```

填写后的两份文件包含凭据，保留原环境副本供后续升级；生成内容不要输出到公共日志。

### 6.7 整体导入版本化配置

执行位置：有 kubectl 的运维机。`NS` 和 `GATEWAY_NS` 使用第 5 节的实际 namespace。先生成文件，再分别校验、应用：

```bash
kubectl -n "$NS" create secret generic "spacegate-admin-config-$VERSION" --from-env-file="$RELEASE_DIR/config/spacegate-admin.env" --dry-run=client -o yaml > "$PRIVATE_DIR/admin-config.yaml"
kubectl -n "$GATEWAY_NS" create secret generic "spacegate-config-$VERSION" --from-env-file="$RELEASE_DIR/config/spacegate.env" --dry-run=client -o yaml > "$PRIVATE_DIR/gateway-config.yaml"
kubectl apply --dry-run=server -f "$PRIVATE_DIR/admin-config.yaml"
kubectl apply --dry-run=server -f "$PRIVATE_DIR/gateway-config.yaml"
kubectl apply -f "$PRIVATE_DIR/admin-config.yaml"
kubectl apply -f "$PRIVATE_DIR/gateway-config.yaml"
```

每份服务配置整体存入一个 K8s Secret，工作负载通过一个 `envFrom` 引用加载，无需区分安全变量。不同 namespace 自动生成各自对象，不做跨 namespace 引用；Admin 不读取网关 AWS 凭据。仅内存 Body 模式可删除网关文件中的 AWS 两项，token 仍保留。

本包补丁以 `v2026.11.20-queue-001` 为版本替换标记，交付手册会替换为本次 VERSION；资源名与镜像 tag 必须对应。保留旧配置对象和 revision 供回滚；后续只更新镜像可继续引用已有配置。更新对象本身不会改变运行进程环境，必须 rollout 重建 Pod 才生效。

## 7. K8s 配置修改与首次 Admin 切换

### 7.1 备份当前工作负载与配置资源

执行位置：目标运维机；已设置第 5、6 节变量。`jq` 用于去掉服务端生成字段，备份保存在私有目录。

```bash
export OLD_ADMIN_REVISION="$(kubectl -n "$NS" get deployment "$ADMIN_DEPLOYMENT" -o jsonpath='{.metadata.annotations.deployment\.kubernetes\.io/revision}')"

kubectl -n "$NS" get deployment "$ADMIN_DEPLOYMENT" -o json \
  | jq 'del(.status,.metadata.managedFields,.metadata.uid,.metadata.resourceVersion,.metadata.creationTimestamp,.metadata.generation,.metadata.annotations["kubectl.kubernetes.io/last-applied-configuration"])' \
  > "$PRIVATE_DIR/admin.before.json"
kubectl -n "$GATEWAY_NS" get daemonset "$GATEWAY_DAEMONSET" -o json \
  | jq 'del(.status,.metadata.managedFields,.metadata.uid,.metadata.resourceVersion,.metadata.creationTimestamp,.metadata.generation,.metadata.annotations["kubectl.kubernetes.io/last-applied-configuration"])' \
  > "$PRIVATE_DIR/gateway.before.json"

kubectl -n "$NS" rollout history deployment/"$ADMIN_DEPLOYMENT" > "$PRIVATE_DIR/admin-history.txt"
kubectl -n "$GATEWAY_NS" rollout history daemonset/"$GATEWAY_DAEMONSET" > "$PRIVATE_DIR/gateway-history.txt"
printf '%s\n' "$OLD_ADMIN_REVISION" > "$PRIVATE_DIR/admin-before-revision.txt"
```

从 `gateway-history.txt` 记录当前网关 revision，后续回滚必须明确选它，不盲用“上一版”。保留原镜像、引用的 Secret/ConfigMap、Ingress、Service 和已安装的 CRD/RBAC。通过集群既有备份方案备份 Gateway/路由/过滤器资源，以及数据库、Redis 和对象存储必要数据；不要把全量 Secret 输出到公共制品目录。

### 7.2 检查旧资源与插件文件依赖

```bash
kubectl get crd gateways.gateway.networking.k8s.io \
  httproutes.gateway.networking.k8s.io \
  sgfilters.spacegate.idealworld.group \
  httpspaceroutes.spacegate.idealworld.group \
  mcproutes.spacegate.idealworld.group \
  wasmplugins.extensions.higress.io

kubectl -n "$NS" get sgfilters.spacegate.idealworld.group -o yaml \
  > "$PRIVATE_DIR/sgfilters.before.yaml"
kubectl -n "$NS" get wasmplugins.extensions.higress.io -o yaml \
  > "$PRIVATE_DIR/wasmplugins.before.yaml"
rg -n 'file:|\.wasm|\.so|ai-gateway-queue|18080' \
  "$PRIVATE_DIR/sgfilters.before.yaml" "$PRIVATE_DIR/wasmplugins.before.yaml"
```

最后一条 `rg` 没有匹配时退出码 1 表示“未发现这些引用”，不是自动失败；仍核对其余插件绑定。文件可能含私有插件配置，不对外分享。

| 实际引用 | 要求 |
| --- | --- |
| `/lib/spacegate/wasm/spacegate_plugin_ai_gateway_queue.wasm` | 纯网关镜像不含它；先提供兼容制品或迁移旧绑定。旧队列后端依赖也同时核对 |
| `/plugins/<name>.wasm` | 保留各节点 `/opt/spacegate/wasm` 等实际来源与挂载，校验文件哈希一致 |
| 第三方 native dylib | 同 OS/架构仍不足以证明 Rust ABI 兼容；按原插件发行约束验证并保留目录 |
| 内置 `hai-quota` | 不需要单独插件文件，检查新版网关的插件目录和 Schema |

当前本地 `/plugins/spacegate_wasm_hello.wasm` 和 `/plugins/hai_process_mix.wasm` 来自挂载卷；这不表示测试集群必须复制它们。只交付目标配置实际需要的已验收文件。不要导出整个本地 volume 替换目标节点目录。

CRD 缺失时参考 [installation.md](installation.md)，按当前资源模型单独评估安装；不要为更新镜像重新应用全部基础模板，导致覆盖现场镜像、权限或业务配置。

### 7.3 准备新 Admin 的完整目标清单

```bash
kubectl set image --local -f "$PRIVATE_DIR/admin.before.json" \
  "$ADMIN_CONTAINER=$ADMIN_IMAGE" -o yaml > "$PRIVATE_DIR/admin.desired.yaml"
```

用编辑器修改 `admin.desired.yaml`。它来源于现场 Deployment，保留现有 selector、标签、ServiceAccount、RBAC、调度约束、资源限制和 imagePullSecrets。不要直接用以下片段替代完整文件。

逐项修改：

1. 删除旧 `/etc/nginx/nginx.conf` 的 `nginx-config` volumeMount；没有其他使用方后再删除对应 volume。旧 ConfigMap 可保留供回滚，不再挂到新后台。
2. 移除旧 `command` / `args` 启动覆盖，使用镜像 ENTRYPOINT。新路径不是 `/usr/src/app/admin-server`。
3. 合并下面的 envFrom，保留已有仍需的引用。删除 `env` 中与新 envFrom 冲突的旧变量：显式 `env` 优先于 envFrom，否则会悄悄继续连接旧数据库/旧 target。
4. 在 `spacegate-admin.env` 设置实际 `CONFIG`、`GATEWAY_CLASS_NAME`、`GATEWAY_INSTANCE`，对外端口为 9080；内部 HOST/PORT 由镜像入口处理。
5. 首轮保留单副本，配置启动/就绪/存活探针。`terminationGracePeriodSeconds` 按现有任务退出预算确定，示例 60 秒。

当前合并入口先以 root 创建/调整运行目录，再通过 gosu 以 node 用户启动 Node；Nginx/Rust 也由同一监管入口拉起。若现场有 `runAsNonRoot`、固定 `runAsUser` 或只读根文件系统约束，先核验这些启动和目录写入是否兼容；不兼容时需另行适配镜像/目录挂载，不能仅更改 image 宣称支持，也不要临时关闭集群安全策略掩盖问题。

```yaml
# 合并到实际 Deployment 中；版本名、namespace、class、instance 必须替换。
spec:
  replicas: 1
  template:
    spec:
      serviceAccountName: spacegate-admin
      terminationGracePeriodSeconds: 60
      containers:
        - name: spacegate-admin
          image: spacegate-admin:v2026.11.20-queue-001
          imagePullPolicy: IfNotPresent
          envFrom:
            - secretRef:
                name: spacegate-admin-config-v2026.11.20-queue-001
          ports:
            - name: http
              containerPort: 9080
          startupProbe:
            httpGet: {path: /ready, port: 9080}
            periodSeconds: 5
            failureThreshold: 60
          readinessProbe:
            httpGet: {path: /ready, port: 9080}
            periodSeconds: 10
          livenessProbe:
            httpGet: {path: /health, port: 9080}
            periodSeconds: 20
```

文件须用本次实际变量值替换示例值，不能把 shell `$VERSION` 原样写进 YAML 并期望 K8s 展开。已有新的合并 Admin 时，不再重复“首次”删除或生成用户，保留已验收配置。

### 7.4 准备网关目标清单

```bash
kubectl set image --local -f "$PRIVATE_DIR/gateway.before.json" \
  "$GATEWAY_CONTAINER=$SPACEGATE_IMAGE" -o yaml > "$PRIVATE_DIR/gateway.desired.yaml"
```

编辑现场派生的 `gateway.desired.yaml`，保留 hostNetwork、DNS、ServiceAccount、业务端口、OTel、证书、插件文件、节点调度及其他原有设置。关键字段示例：

```yaml
# 合并到实际 DaemonSet template；不是完整资源。
spec:
  template:
    spec:
      terminationGracePeriodSeconds: 60
      containers:
        - name: spacegate
          image: spacegate:v2026.11.20-queue-001
          imagePullPolicy: IfNotPresent
          envFrom:
            - secretRef:
                name: spacegate-config-v2026.11.20-queue-001
```

在 `spacegate.env` 设置 `STARTUP_MODE=k8s`、`CONFIG=k8s:<namespace>`、`K8S_NAMESPACE`、`GATEWAY_CLASS_NAME` 与 `PLUGIN_DIR`。清除显式旧 env 中所有与该文件重复的键，包括 `CONFIG_FILE_PATH`，避免覆盖整体导入配置；保留原有其他必要 envFrom。不要向 HAI 网关追加旧 CLI 的 `args: ["-c", "k8s:..."]` 作为配置方式。

插件目录可以不同，但目录必须存在且与 PLUGIN_DIR 一致。新纯网关创建 `/var/lib/spacegate/plugins`、`/plugins`；不能假定还存在旧 `/lib/spacegate/plugins`。`PLUGINS` 是另一入口的参数，不等同于此 HAI 入口的 PLUGIN_DIR。

### 7.5 后续仅更新镜像的简化路径

已经完成首次切换且配置不变时：按第 3、5、7.1 节保存制品和回滚点，确认队列可排空后执行下面需要更新的服务，省略不更新者。

```bash
kubectl -n "$GATEWAY_NS" set image daemonset/"$GATEWAY_DAEMONSET" \
  "$GATEWAY_CONTAINER=$SPACEGATE_IMAGE"
kubectl -n "$GATEWAY_NS" rollout status daemonset/"$GATEWAY_DAEMONSET" --timeout=600s

kubectl -n "$NS" set image deployment/"$ADMIN_DEPLOYMENT" \
  "$ADMIN_CONTAINER=$ADMIN_IMAGE"
kubectl -n "$NS" rollout status deployment/"$ADMIN_DEPLOYMENT" --timeout=600s
```

配置也有变化时使用 7.3、7.4 的完整清单，一次性提交 image 和配置引用，避免多次更新把服务停在半配置状态。不要同时在另一套 GitOps 清单保留旧值；实际通过 GitOps 管理的环境，应把审定后的完整清单交给既有流水线应用。

## 8. 插件与队列专项配置

### 8.1 必须区分的三种名字

| 名称 | 当前本地 | 目标环境要求 |
| --- | --- | --- |
| K8s namespace | 本地非 K8s | CONFIG/K8S_NAMESPACE 的资源范围 |
| Gateway 配置键 / 运行时 gateway.name | `hai-hub` / `hai-hub-dev` | 对应 Admin target 的 gateway / runtimeGateway |
| queue.namespace | `remote-model-s3-test` | Redis 队列隔离域，management scope 必须相同 |

`queue.namespace` 不是 Kubernetes namespace。不同环境不要共享同一个队列 Redis 隔离域。相同 queue.namespace 的副本须使用一致资源策略；不能各自填写不同容量后期望自动合并。

### 8.2 保存、绑定与 S3/OBS

在新 Admin 的插件实例编辑器配置 `hai-quota`，并绑定到实际 `/api/v1` 模型路由链。保留原 `hai-auth`、`hai-asset`、`hai-dispatch`、审计等业务顺序。全局创建实例但未绑定不会生效。

`queue.management` 示例：

```json
{"enabled":true,"token_env":"HAI_QUEUE_ADMIN_TOKEN","operator_id":"test-queue-console","scopes":[{"namespace":"test-model-queue"}]}
```

必须与 `queue.namespace=test-model-queue` 一致；token_env 是环境变量名字，不是密钥值。资源池按模型/资产等维度配置，流式模型以执行并发和等待容量控制；可选 QPS 控制启动速率，不能替代执行槽。

Body 存储有两种独立用途：

| 项目 | 配置位置 | 凭据 |
| --- | --- | --- |
| Admin 头像等资源 | Admin 的 HAI_STORAGE_* | HAI_STORAGE_ACCESSKEYID / SECRETACCESSKEY |
| 排队请求体 | quota 的 queue.body_storage.s3 | 网关 AWS_ACCESS_KEY_ID / AWS_SECRET_ACCESS_KEY |

普通 S3 兼容服务使用 `provider: s3`（省略时默认），endpoint 填服务根地址，bucket/prefix 分开填写。华为 OBS 的网关 Body 存储使用 `provider: huawei_obs`，endpoint 为区域 HTTPS 根地址，例如 `https://obs.cn-north-4.myhuaweicloud.com`，bucket 单独填写，不能重复包含桶名或路径。OBS bucket 当前要求无点号的 3～63 位小写字母/数字/连字符名称。

本地 S3 验收不是云端 OBS 验收；首次上云须实际验证上传、multipart、回读、取消及删除。Admin 通用存储模块没有本插件的 `huawei_obs` 专用开关，不将两者适配能力混为一谈。

`orphan_retention_secs` 不会自动替云端创建 bucket lifecycle。按实际清理和故障恢复窗口设置服务端兜底保留策略，不能让生命周期提前删掉仍在等待/回读的正文；大请求总大小、multipart 并发和网关内存限制需一起核对。

当前本地用于故意覆盖 S3 流程的阈值为 **1 byte**，池容量为 **执行 1 / 等待 5**。这些是功能验收值，不是 50～100 并发的容量建议。目标部署初测可使用小容量；升到目标容量后另做压测和内存/上游预算验收。通常 500 KiB 转存使用 `512000` 字节，500 KB 则是 `500000`，先统一单位。

本文不把容器本地示例自动同步到集群。资产发布侧提供的参数只是部分条件，最终准入还取决于当前生效插件配置、资产上下文和匹配规则；先用后台展示的实际配置核对，不只看环境变量。

### 8.3 首次初始化空队列 namespace

保存、绑定并确认目标进程注册了正确 namespace 后，读取管理快照。仅状态 `uninitialized` 且确认该 namespace 全新/无旧执行与账本时执行 bootstrap。已有 `healthy` 不初始化；已有 `recovery_required` 走 8.4。

以下在运维机通过 port-forward 访问某一个确定的网关 Pod，管理端口以实际为准；不要发布公网 Ingress：

```bash
# 终端 A；替换真实 Pod 名。hostNetwork 网关默认管理端口 9876。
kubectl -n "$GATEWAY_NS" port-forward pod/实际网关Pod名 19876:9876
```

终端 B 重新设置相同 PRIVATE_DIR 与以下实际参数，读取同一 token 文件，不把 token 写进命令行参数：

```bash
export QUEUE_NAMESPACE="test-model-queue"
export RUNTIME_GATEWAY_NAME="实际运行时Gateway名称"
export QUEUE_API_BASE="http://127.0.0.1:19876"

node - "$PRIVATE_DIR/queue-token.txt" "$PRIVATE_DIR/queue-api.headers" "$RUNTIME_GATEWAY_NAME" <<'JS'
const fs = require('node:fs');
const [tokenFile, output, gateway] = process.argv.slice(2);
const token = fs.readFileSync(tokenFile, 'utf8').trim();
if (token.length < 32 || /[\r\n]/.test(token + gateway)) throw new Error('Invalid management identity');
fs.writeFileSync(output, `Authorization: Bearer ${token}\nx-hai-queue-gateway: ${gateway}\nContent-Type: application/json\n`, { mode: 0o600 });
JS

curl --fail-with-body --silent --show-error --max-time 15 \
  --header @"$PRIVATE_DIR/queue-api.headers" \
  "$QUEUE_API_BASE/management/hai-quota/v1/namespaces/$QUEUE_NAMESPACE/snapshot"
```

仅在上面条件确认为空且 uninitialized 后，首次创建并保存 epoch 请求：

```bash
(
  set -eu
  test ! -e "$PRIVATE_DIR/queue-bootstrap.json"
  node -e 'process.stdout.write(JSON.stringify({epoch:require("node:crypto").randomUUID(),expected_state:"uninitialized",confirm_drained:true}))' \
    > "$PRIVATE_DIR/queue-bootstrap.json"
)

curl --fail-with-body --silent --show-error --max-time 15 \
  --header @"$PRIVATE_DIR/queue-api.headers" \
  --data-binary @"$PRIVATE_DIR/queue-bootstrap.json" \
  "$QUEUE_API_BASE/management/hai-quota/v1/namespaces/$QUEUE_NAMESPACE/bootstrap"
```

响应不明时保留原请求文件与 epoch，先核对状态，不能反复生成新 epoch 重试。初始化共享 namespace 只做一次；不为每个副本创建不同 epoch。snapshot 的 healthy 与 local_operational 分别核对：bootstrap 返回 healthy 不等于每个进程已经启动队列调度器；发起受控请求后再次核对各目标。

### 8.4 Redis 重启后的受控恢复

`queue_recovery_required` 不会通过更新镜像、启动 Admin Node scheduler 或清列表解决。保留原 namespace、原历史和 Body，不重放旧请求。

以对应版本的 HAI 源码手册 [hai-quota-controlled-recovery.md](../../../../hai-workspace/hai-hub/docs/hai-quota-controlled-recovery.md) 为准：停止旧 owner、核实无活租约与未清 Body、备份后，按旧 epoch/run_id 与当前 observed_run_id 先 dry-run，再提交固定 operation_id/new_epoch。仅 namespace scope 有恢复权限。拒绝时按返回原因处理，不直接写 Redis control 或删除键绕过。

跨机器交付时该相邻路径可能不存在，另将同版本恢复手册作为非密钥运维资料交付。未知远端执行结果、清理积压或扫描超限均不是“重新初始化即可”的情况。

## 9. 首次发布完整清单与入口

### 9.1 先审查，后应用

```bash
kubectl apply --dry-run=server -f "$PRIVATE_DIR/gateway.desired.yaml"
kubectl apply --dry-run=server -f "$PRIVATE_DIR/admin.desired.yaml"
kubectl diff -f "$PRIVATE_DIR/gateway.desired.yaml"
kubectl diff -f "$PRIVATE_DIR/admin.desired.yaml"
```

`kubectl diff` 退出码 1 表示存在差异，需人工审查；其他失败先处理。server dry-run 验证资源可被 API 接受，不验证数据库可达或模型业务。差异可能含现场私有环境值，只在可信终端审查，不把输出放公共构建日志。

在测试流量窗口停止新增模型请求，确认现有 queued/executing/Body 计量清零后再更新网关。长 SSE 不能依赖默认终止宽限保证全部完成；保留足够 shutdown 时间，不能强杀作为正常升级步骤。首次 K8s/多副本尚未整体验收时，不宣称此流程保证零中断。

```bash
kubectl apply -f "$PRIVATE_DIR/gateway.desired.yaml"
kubectl -n "$GATEWAY_NS" rollout status daemonset/"$GATEWAY_DAEMONSET" --timeout=600s

kubectl apply -f "$PRIVATE_DIR/admin.desired.yaml"
kubectl -n "$NS" rollout status deployment/"$ADMIN_DEPLOYMENT" --timeout=600s
```

只更新一个组件时执行其对应清单，不要求无关组件也重启。若 DaemonSet 使用 OnDelete 策略，rollout 不会自动替换全部 Pod，须按现场策略逐节点排空、替换和验收，不能把模板 image 已变当作所有节点已更新。

### 9.2 Service / Ingress

- Admin Service 继续 `port: 9080`、`targetPort: 9080`，selector 匹配现场 Deployment 标签。
- Ingress 指向该 Service，推荐 HTTPS 域名根路径；保留 `/api/v1/...`，删除旧 `/api` 剥离规则。
- 不直接把 `/api` 转发到 Rust:9081；不暴露网关 `/management/hai-quota/...` 给公网。
- `PUBLIC_API_BASE` 留空，页面请求使用当前 Origin；默认不支持把此根路径镜像随意挂到新子路径而不做额外适配。
- 保留模型入口原路由；其代理超时应覆盖允许的等待与执行时间，SSE 禁止不合适的响应缓冲。Admin 的请求体 2 MiB 限制针对后台 API，不是模型大请求的转存上限。

Ingress/Service 有变更时单独备份、diff、应用现场派生清单。若仅替换镜像且入口已符合要求，无须修改。

## 10. 发布后验收与放量

### 10.1 运行与镜像身份

```bash
kubectl -n "$NS" get pods -o wide
kubectl -n "$GATEWAY_NS" get pods -o wide
kubectl -n "$NS" logs deployment/"$ADMIN_DEPLOYMENT" --tail=100
kubectl -n "$NS" get pods -o json \
  | jq '.items[] | {pod:.metadata.name,images:[.status.containerStatuses[]? | {name,image,imageID,ready,restartCount}]}'
```

网关 namespace 不同时同样执行该 imageID 查询。核对全部实际副本，而不只看 Deployment/DaemonSet 模板。运行时 imageID 可能是 registry manifest digest，不必与 Docker config image ID 字符串相等；结合制品来源、实际镜像引用和运行时内容身份核对。

### 10.2 Admin 与网关基础验收

1. `/health`、`/ready` 成功。可用 `kubectl port-forward service/spacegate-admin 19080:9080` 本地检查；后台生产登录仍走 HTTPS 入口以验证 Secure Cookie。
2. 新管理员登录；刷新页面、退出再登录、审计列表正常。旧 Vue ak/sk 不是新 IAM 账号。已有账号迁移场景要验证原密码、角色和历史审计，不能用新空库登录代替。
3. 网关实例健康、插件目录和 hai-quota Schema 正常；只看到名称不能代替 Schema 中队列字段存在。
4. 独立临时测试路由完成创建/读取/更新、配置监听/reload、经网关真实调用，随后清理测试资源；不要修改正式路由当作 smoke。
5. 确认普通用户不能执行管理写操作，内部 9081/网关管理端口没有通过公网暴露。

### 10.3 原生队列专项验收

1. 配置实例与绑定正确，目标列表地址正确；逐个运行目标核对 saved version、config_matches、healthy 与 local_operational。
2. 先使用受控延时模型，发送超过执行容量的请求，观察执行数、等待数；再覆盖等待满、等待超时、客户端取消。
3. 确认被拒绝/取消/超时的等待请求没有派发上游，后续请求能复用槽位。
4. SSE 持续期间仍占用执行槽，结束/断开后按语义释放；正式远程模型用小量请求验证，避免无意义并发触发上游限流。
5. 打开 S3/OBS 时验证实际上传、回读、清理及 multipart 取消；最后 queued/executing/Body managed/unassigned 回到零，无遗留测试上游活请求。
6. `request_id` 与 Collector/ClickHouse 访问审计可关联；请求表显示实际模型。管理历史不是执行租约，历史“已失联”不代表仍占用执行槽。

本版本已知边界：部分强杀留下的失联管理记录可能没有 TTL，不能把“十分钟后列表全空”作为验收前提，也不能为清展示删除队列账本。云 OBS、K8s 和多副本必须以目标环境实测结果记录；本地 Docker 测试不能自动替代。

### 10.4 完成记录

记录发布版本、实际 imageID/digest、配置对象名、Gateway/queue.namespace、全部副本状态、测试 request_id、验收时间和未完成项。记录放私有运维目录；脱敏结果可随制品保存。验收不通过不放量，转 11/12 节。

## 11. 回滚

先停止新增测试请求、确认现有执行与 Body 状态，明确回滚范围。回滚不删除 Admin 数据库、Redis、S3 或 epoch。

### 11.1 工作负载模板回滚

Admin 使用第 7.1 节记录的 revision：

```bash
export OLD_ADMIN_REVISION="$(cat "$PRIVATE_DIR/admin-before-revision.txt")"
kubectl -n "$NS" rollout undo deployment/"$ADMIN_DEPLOYMENT" --to-revision="$OLD_ADMIN_REVISION"
kubectl -n "$NS" rollout status deployment/"$ADMIN_DEPLOYMENT" --timeout=600s
```

网关从本次备份的 history 中选定发布前 revision，再执行：

```bash
export OLD_GATEWAY_REVISION="填写已记录的发布前revision"
kubectl -n "$GATEWAY_NS" rollout undo daemonset/"$GATEWAY_DAEMONSET" --to-revision="$OLD_GATEWAY_REVISION"
kubectl -n "$GATEWAY_NS" rollout status daemonset/"$GATEWAY_DAEMONSET" --timeout=600s
```

回滚 revision 会恢复工作负载 template 中的镜像、env、挂载引用，不会恢复被原地覆盖的 Secret、ConfigMap、Ingress 或业务 CRD。因此本手册使用版本化配置并保留旧对象。首次回旧 Admin 时，原 revision 中的旧 Nginx volumeMount 及旧 ConfigMap 必须仍可用。

### 11.2 数据与入口

若本次改变 Ingress/Service，按审定的发布前备份单独恢复并验证。新版本如修改持久 schema/队列协议，先检查旧二进制兼容性；不能认为 rollout undo 等于数据回滚。历史数据恢复是独立的数据操作，按备份与迁移方案执行，不在这里自动覆盖。

## 12. 常见失败与定位顺序

| 现象 | 优先核对 |
| --- | --- |
| ImagePullBackOff / ErrImageNeverPull | 镜像是否进入实际 CRI、完整镜像名、节点架构、拉取策略及仓库凭据 |
| 新后台页面出现但登录/加密 API 404 | 旧 nginx-config 挂载、Ingress 剥离 /api、PUBLIC_API_BASE 填了 /api/v1 |
| Admin 启动立即退出 | CONFIG、密钥环、ADMIN SK、JWT key、数据库与存储、production 通知 Provider；看具体子进程错误 |
| 改了 ConfigMap 仍连旧地址 | Pod 未重建，或者显式 env 覆盖 envFrom；非空 DB/CACHE URL 覆盖分项配置 |
| 登录成功但刷新失效 | 共享 Redis/密钥环、Cookie、HTTPS、域名和跨域配置；不要通过关闭认证解决 |
| 实例发现/reload 失败 | ServiceAccount/RBAC、GatewayClass、GATEWAY_INSTANCE、hostNetwork 与内部管理端口 |
| Wasm 文件不存在 | 完整 K8s 镜像内置文件与纯网关差异、hostPath 文件、实际 URL 和每节点文件一致性 |
| 第三方 .so 未加载 | PLUGIN_DIR、目录存在、Linux 架构及 Rust ABI；HAI 内置插件不走此目录 |
| 队列页无目标 / 未绑定 | NATIVEQUEUETARGETS 数组、gateway 配置键、实际路由绑定 |
| 队列管理 401/403 | 两进程 token、token_env 名称、namespace/pool scope、runtimeGateway |
| queue_recovery_required | Redis run_id/健康条件与原 epoch，执行受控恢复，不能清 Redis |
| queue_config_conflict | 同 namespace 资源策略冲突；先排空，按策略迁移方案处理，不靠重复重启 |
| S3/OBS 请求失败 | 网关 AWS_* 与插件 endpoint/provider/bucket/region、DNS/TLS、权限；Admin HAI_STORAGE_* 不是替代 |
| 模型请求等待后断开 | 调用方/入口代理超时、队列等待上限、取消状态与 Body 清理；HTTP 200 还需检查模型正文 |
| 管理列表旧记录不消失 | 历史 TTL/失联分支，不等于实时槽位；不能删除租约/Body 账本来清列表 |

## 13. 源码与资料入口

- [旧服务镜像 Runbook](service-image-build-runbook.md)：旧 Vue / Wasm 流程历史参考。
- [新 Admin README](../../../spacegate-admin/README.md)：当前模块配置、构建和运行要求。
- [新 Admin 合并 Dockerfile](../../../spacegate-admin/images/combined.Dockerfile)。
- [新 Admin 启动脚本](../../../spacegate-admin/images/combined/start.sh)。
- [纯 HAI 网关 Dockerfile](../../resource/docker/spacegate-runtime/hai.Dockerfile)。
- [完整 K8s 网关 Dockerfile](../../resource/docker/spacegate-k8s/Dockerfile)。
- [原 Admin K8s 模板](../../resource/kube-manifests/spacegate-admin-server.yaml)：仍含旧 Nginx 挂载，不原样用于新 Admin。
- [网关 K8s 模板](../../resource/kube-manifests/spacegate-gateway.yaml)：用于对照现有 CRD、RBAC、网络和挂载，不整份覆盖现场。
- [受控恢复说明](../../../../hai-workspace/hai-hub/docs/hai-quota-controlled-recovery.md)。

本文编写时已核对本地镜像、启动入口和配置格式；文档命令的静态校验不等于已经执行目标集群发布。每次发布按第 10 节留下实际验收记录。

## 附录 A. 当前本地 hai-quota 配置（脱敏参照）

以下来自本地挂载的插件实例配置，不是目标 K8s 配置，也不包含整个 Gateway 的 Redis/鉴权配置。未显式设置的字段由该版本 Schema/Rust 默认值处理；这里保留本地显式 runtime 值以便核对格式。

```json
{
  "code": "hai-quota",
  "kind": "named",
  "name": "hai-quota-3772b6078bcd517c",
  "spec": {
    "queue": {
      "enabled": true,
      "namespace": "remote-model-s3-test",
      "runtime": {
        "max_waiters_per_instance": 1000,
        "max_active_groups_per_instance": 4096,
        "max_wait_ms": 60000,
        "heartbeat_ms": 1000,
        "waiter_lease_ms": 5000,
        "poll_interval_ms": 100,
        "redis_rpc_timeout_ms": 1000,
        "revalidate_timeout_ms": 2000,
        "proof_valid_ms": 1000,
        "execution_lease_ms": 15000,
        "execution_safety_margin_ms": 3000,
        "prepare_claim_ms": 3000,
        "max_preparing_per_group": 16,
        "max_revalidations_per_instance": 32,
        "scheduler_drain_batch_size": 16,
        "heartbeat_batch_size": 128,
        "grant_to_dispatch_timeout_ms": 5000,
        "shutdown_drain_ms": 5000
      },
      "body_storage": {
        "enabled": true,
        "offload_threshold_bytes": 1,
        "max_body_bytes": 536870912,
        "max_buffer_bytes_per_instance": 67108864,
        "max_staging_requests_per_instance": 32,
        "max_staged_bytes_per_pool": 1073741824,
        "max_unassigned_staged_bytes_per_namespace": 1073741824,
        "max_managed_object_bytes_per_namespace": 2147483648,
        "receive_upload_timeout_ms": 120000,
        "restore_open_timeout_ms": 2000,
        "restore_timeout_ms": 300000,
        "offload_on_memory_pressure": true,
        "s3": {
          "endpoint": "http://host.docker.internal:19000",
          "region": "us-east-1",
          "bucket": "hai-quota-test",
          "prefix": "hai-quota/remote-model-config-test",
          "credentials_source": "environment",
          "multipart_part_bytes": 5242880,
          "max_parallel_uploads_per_instance": 4,
          "max_inflight_parts_per_upload": 1,
          "orphan_retention_secs": 86400
        }
      },
      "management": {
        "enabled": true,
        "token_env": "HAI_QUEUE_ADMIN_TOKEN",
        "operator_id": "local-config-test",
        "scopes": [
          {
            "namespace": "remote-model-s3-test"
          }
        ]
      },
      "pools": [
        {
          "id": "remote-model-test-pool",
          "key_by": "fixed",
          "max_concurrent": 1,
          "qps_limit": null,
          "max_waiters": 5,
          "max_wait_ms": 30000,
          "default_policy": "wait",
          "allow_client_abandon": true,
          "allowed_values": [],
          "model_aliases": {},
          "unknown_value": "reject"
        }
      ],
      "rules": [
        {
          "id": "remote-model-config-test-rule",
          "match": {
            "asset_ids": [
              "A4fisBzFhgZKJG5160ahK"
            ],
            "asset_types": [],
            "path_prefixes": [],
            "methods": [],
            "request_models": [],
            "effective_models": []
          },
          "action": "queue",
          "pool": "remote-model-test-pool"
        }
      ]
    }
  }
}
```

本地核对重点：

- `offload_threshold_bytes=1` 是强制覆盖转存路径的测试值，不能误当作常规默认的 500 KiB。
- `max_body_bytes=536870912` 为 512 MiB，`max_buffer_bytes_per_instance=67108864` 为 64 MiB；请按目标 Pod 内存和并发一起核对。
- 池中 `max_wait_ms=30000`（30 秒）比 runtime 全局的 60000 更小；本池不能据全局值推断等待 60 秒。
- `qps_limit=null` 表示本池未设置可选启动 QPS 限制，并非并发无限。
- `key_by=fixed` 让该规则命中的请求共享一组容量；这里不是按请求体模型名分别建队列。
- 规则仅匹配示例资产 `A4fisBzFhgZKJG5160ahK`；测试环境需使用其自身已发布资产与授权 Key，不能只复制资产 ID。
- management scope 与 queue.namespace 完全相同；bucket 名与 Admin 的 `spacegate-admin-hub-test` 不同。
- `s3.provider` 在本地 JSON 中省略，按当前默认 S3 处理；切 OBS 需显式选择 `huawei_obs` 并替换 endpoint、region、bucket。
- 顶层 `redis_url` 未设置时使用 Gateway 级 Redis；不能仅凭此 JSON 检查完整 Redis 连接和权限。
- 本附录不含 token、AWS 凭据或原资产 Key；这些仅在运行时私有环境中提供。
