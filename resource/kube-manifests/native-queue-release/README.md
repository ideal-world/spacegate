# 新 Admin / 原生队列 K8s 发布文件包

适用于已有 SpaceGate K8s 环境的测试发布，默认沿用 `namespace=spacegate`、Admin Deployment/容器 `spacegate-admin`、网关 DaemonSet/容器 `spacegate`。文件内含明确占位符，填写后使用，不能原样发布。

## 运维先看哪些文件

| 文件 | 用途 | 现场区别 |
| --- | --- | --- |
| `runtime-config.yaml` | ConfigMap + Admin Secret + 队列 Secret | 配置集中填写；共享队列 token 只填一次，Admin 不读取网关 AWS 凭据 |
| `admin.patch.yaml` | 已有 Admin 的 strategic merge 补丁 | 换镜像、引用集中配置、删除旧 Nginx 挂载和冲突 env，换健康探针；保留其他现场设置 |
| `gateway.patch.yaml` | 已有网关的 strategic merge 补丁 | 换镜像、指定 K8s 模式、引用 token/AWS；不改变业务路由、插件卷和 OTel |
| `admin-deployment.example.yaml` | 新安装/对照的完整 Deployment + Service | 仅在没有旧后台时使用；需已有 ServiceAccount/RBAC，不是空集群安装方案 |

**不要 `kubectl apply -f 本目录`。两个 `.patch.yaml` 不是完整资源。** 已有部署使用补丁；新部署使用完整 example，二选一。详细备份、镜像导入、队列初始化及回滚见 [发布手册](../../../docs/k8s/native-queue-release-runbook.md)。

**希望直接编辑本地 env 文件时：** 使用 [Admin 按环境配置模板](../../../../spacegate-admin/deploy/k8s/README.md)，通用项已复制，环境项和私密项分别标注。它生成与本包同名的 ConfigMap/Secret，可直接配合两个补丁；无需重新构建镜像。该方式替代下文填写/应用 `runtime-config.yaml` 的步骤，不同时维护两份配置来源。

## 1. 复制并集中填写

将目录复制到私有运维目录，真实密码只写副本，不提交回仓库。以下命令在有目标集群 kubeconfig 的运维机执行：

```bash
umask 077
export RELEASE_DIR="$HOME/.local/state/spacegate-k8s/v2026.11.20-queue-001"
mkdir -p "$RELEASE_DIR"
# 路径换为运维机收到的本文件包目录。
cp /收到的文件包目录/*.yaml "$RELEASE_DIR/"
chmod 600 "$RELEASE_DIR/"*.yaml

export NS=spacegate
export GATEWAY_NS=spacegate
export ADMIN_DEPLOYMENT=spacegate-admin
export GATEWAY_DAEMONSET=spacegate
kubectl config current-context
```

填写内容集中在 `runtime-config.yaml`：

- 替换全部 `REPLACE_*`：数据库/Redis 地址、对象存储、后台域名、网关目标及密钥。无 Redis 密码时写 `""`，不要保留占位词。
- ConfigMap 中按现场修改 `CONFIG`、GatewayClass、DaemonSet.namespace；target 的 gateway 是配置键，runtimeGateway 是运行时名，二者不能由 K8s namespace 猜测。
- Secret 中 SM2 key/JWT 等按手册生成或复用受管原件；已有部署不得为每次升级重新生成。镜像 tag 和三个版本化配置对象名在各文件中必须一致。
- 当前本地配置在文件注释中标出，完整脱敏对照见手册第 6 节、附录 A。`host.docker.internal`、本地 bucket 和测试资产不要复制到目标。
- 此包显式使用 `NODE_ENV=test`、`HAI_CORE_ENV=test`；生产要同时改为 production 并填写真实通知 Provider。不能凭此模板声称完成生产模式验收。

其他需要核对的差异：

- 两个补丁内的容器名、镜像引用、GatewayClass/namespace/PLUGIN_DIR；不同命名空间时拆开 runtime-config.yaml 的资源并按各自 namespace 创建对应 Secret。Admin 和网关都需其本 namespace 可访问的同值 token。
- 离线镜像先导入每个实际调度节点的 CRI；默认 imagePullPolicy 为 IfNotPresent。用仓库时将两个 image 改为完整仓库名。
- `admin.patch.yaml` 的 envFrom 会替换原 envFrom 列表。若原列表有其他必要引用，将其一并保留在补丁中；未知 env、resources、额外卷等仍按键合并保留。
- 原仓库模板的旧卷名为 nginx-config，挂载路径为 /etc/nginx/nginx.conf；实际不同先调整删除项。若还有其他挂载引用该卷，不可直接删除 volume。
- Admin 补丁有意删除集中配置的同名旧 env，防止其覆盖 envFrom。若删除列表中的某项是现场刻意保留的值，将它移动到集中 ConfigMap/Secret 中，避免两处同时维护。
- 网关补丁清除旧 command/args，使用 HAI 镜像入口；现场若有必要的包装脚本先核验。现有 hostNetwork、RBAC、证书、Wasm 文件等不由本包重建。
- 仅内存 Body 模式不需要 AWS 凭据时，删除 gateway 补丁中的 AWS 两项和队列 Secret 中对应两项；queue token 仍保留。临时 AWS 凭据需要时按注释同时增加 AWS_SESSION_TOKEN 引用。

确认全部占位符已替换（`rg` 没有匹配时退出码 1 表示没有占位符）：

```bash
rg -n 'REPLACE_' "$RELEASE_DIR/"*.yaml
```

真实配置副本不要输出到公共日志。原生 quota 的池/规则/Body 配置仍保存为现有网关插件资源，这些 Secret 不会自动创建或绑定队列；先核对现有插件，再按手册第 8 节配置。

## 2. 一次生成完整差异，不逐个添加变量

先按手册备份现场、记录回滚 revision、保留旧配置对象，停止新增测试流量并确认 queued/executing/Body 归零。下面读取现有工作负载，在本地合并补丁；不会修改集群：

```bash
kubectl -n "$NS" get deployment "$ADMIN_DEPLOYMENT" -o yaml > "$RELEASE_DIR/admin.before.yaml"
kubectl -n "$GATEWAY_NS" get daemonset "$GATEWAY_DAEMONSET" -o yaml > "$RELEASE_DIR/gateway.before.yaml"

kubectl patch --local -f "$RELEASE_DIR/admin.before.yaml" \
  --type=strategic --patch-file "$RELEASE_DIR/admin.patch.yaml" \
  -o yaml > "$RELEASE_DIR/admin.after.yaml"
kubectl patch --local -f "$RELEASE_DIR/gateway.before.yaml" \
  --type=strategic --patch-file "$RELEASE_DIR/gateway.patch.yaml" \
  -o yaml > "$RELEASE_DIR/gateway.after.yaml"

diff -u "$RELEASE_DIR/admin.before.yaml" "$RELEASE_DIR/admin.after.yaml"
diff -u "$RELEASE_DIR/gateway.before.yaml" "$RELEASE_DIR/gateway.after.yaml"
```

`diff` 退出码 1 表示存在差异。运维可直接审查 old/new；重点核对旧 Nginx 删除、镜像、envFrom、引用 Secret 及所有保留项。不将 after.yaml 作为本包主发布入口，以免混淆服务端生成字段和字段所有权。

## 3. 审查后发布

先做 API server dry-run；它不验证数据库可达或模型功能，也不读取本地 Secret 的真实有效性。

```bash
kubectl apply --dry-run=server -f "$RELEASE_DIR/runtime-config.yaml"
kubectl -n "$NS" patch deployment "$ADMIN_DEPLOYMENT" \
  --type=strategic --patch-file "$RELEASE_DIR/admin.patch.yaml" --dry-run=server
kubectl -n "$GATEWAY_NS" patch daemonset "$GATEWAY_DAEMONSET" \
  --type=strategic --patch-file "$RELEASE_DIR/gateway.patch.yaml" --dry-run=server
```

应用配置和需要更新的补丁；只更新 Admin 时省略网关补丁，继续复用既有网关凭据和队列状态。

```bash
kubectl apply -f "$RELEASE_DIR/runtime-config.yaml"

kubectl -n "$GATEWAY_NS" patch daemonset "$GATEWAY_DAEMONSET" \
  --type=strategic --patch-file "$RELEASE_DIR/gateway.patch.yaml"
kubectl -n "$GATEWAY_NS" rollout status daemonset/"$GATEWAY_DAEMONSET" --timeout=600s

kubectl -n "$NS" patch deployment "$ADMIN_DEPLOYMENT" \
  --type=strategic --patch-file "$RELEASE_DIR/admin.patch.yaml"
kubectl -n "$NS" rollout status deployment/"$ADMIN_DEPLOYMENT" --timeout=600s
```

Secret/ConfigMap 名采用版本化，保留旧对象供 rollout undo 使用；后续只换镜像、不改配置时继续引用已验收配置，不必创建新密钥。修改并应用现有 Secret 不会自动更新已经运行的进程环境。

Service 保持原 9080，无须替换；Ingress 必须保留 `/api/v1/...`，不再剥离 `/api` 或转发 Rust:9081。需要改变入口时独立审查现场 Ingress，不在此包猜测域名、TLS Secret 和控制器。

发布后执行手册第 10 节：登录/刷新、插件发现/配置读写、逐副本队列状态、受控延时请求、取消/超时、S3 回读清理、访问审计。Pod Ready 不等于业务链路验收。需要回滚时使用手册第 11 节的发布前 revision，不删数据库/Redis/S3。

## 文件包边界

- 只提供已有集群升级与新 Admin 对照，不重复创建 namespace、CRD、GatewayClass、RBAC 或整个网关 DaemonSet。
- 现有网关的旧 Wasm 路径必须先核对：当前本地纯网关没有 `/lib/spacegate/wasm/spacegate_plugin_ai_gateway_queue.wasm`。外部插件卷不会随镜像 tar 交付。
- 当前 Admin 合并入口有 root 目录准备/gosu 要求；受限 non-root/只读根部署需另行适配。
- 已本地校验 YAML 与补丁合并；未对目标 K8s 集群实际发布或做 server dry-run。目标尚无 kubeconfig 验收记录。
