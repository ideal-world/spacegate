# 新 Admin / 原生队列 K8s 发布文件包

每个服务只交付一份完整配置：`config/spacegate-admin.env` 和 `config/spacegate.env`。普通参数、密码、token、密钥都在对应服务文件中，按 `[需替换]` / `[需核对]` 修改，不再分开维护安全变量。模板见 [配置说明](../../../../spacegate-admin/deploy/k8s/README.md)，完整交付步骤见工作区根目录的 [镜像导出与配置交付](../../../../镜像导出与配置交付.md)。

本文件包适用于已有 K8s 环境，默认 namespace 为 `spacegate`，Admin Deployment/容器为 `spacegate-admin`，网关 DaemonSet/容器为 `spacegate`。队列规则、容量、Redis、S3 endpoint/bucket 等仍在现有网关/`hai-quota` 插件配置中维护。镜像和本文件包不自动创建或绑定业务队列。

| 文件 | 用途 |
| --- | --- |
| `admin.patch.yaml` | 旧/新 Admin 升级：换镜像、整体引用 Admin 配置、清除冲突旧 env 和旧 Nginx 挂载、更新健康探针 |
| `gateway.patch.yaml` | 网关升级：换镜像、整体引用网关配置、清除被接管字段的显式旧 env |
| `admin-deployment.example.yaml` | 没有旧后台时的完整 Deployment + Service 对照，需已有 ServiceAccount/RBAC |

**两个 `.patch.yaml` 必须通过 `kubectl patch --type=strategic --patch-file` 使用，不能直接 apply，也不能 apply 整个文件包。** 完整 example 与升级补丁二选一。旧版 `runtime-config.yaml` 和四份拆分 env 已由两份单服务配置替代，不能混用新旧引用。

## 1. 核对交付配置

下面从收到的完整交付目录执行。该目录含 `VERSION`、`config/`、`k8s/` 和镜像 tar；生成版本的方法见根目录交付手册。若只收到源码模板，先按配置说明准备两份 env，再按交付手册生成对应版本的补丁。

```bash
export RELEASE_DIR=/收到的交付目录
export VERSION="$(cat "$RELEASE_DIR/VERSION")"
export NS=spacegate
export GATEWAY_NS=spacegate
export ADMIN_DEPLOYMENT=spacegate-admin
export GATEWAY_DAEMONSET=spacegate
kubectl config current-context
```

按注释填写两份文件，重点核对：

- 数据库、Redis、对象存储、后台域名和逐网关进程管理地址；两个文件的 `HAI_QUEUE_ADMIN_TOKEN` 必须完全一致。示例 DNS、IP、AK/SK 不是已创建的真实资源。
- Admin 的 `CONFIG`、`GATEWAY_CLASS_NAME`、`GATEWAY_INSTANCE` 和网关启动配置必须符合现场 namespace、GatewayClass、DaemonSet。target 的 gateway/runtimeGateway 是配置名/运行时名，不能按 namespace 猜测。
- 升级复用原有 SM2 密钥环、JWT 签名密钥、内部凭据及 token；首次部署的正式密钥按要求替换示例。生产需两项环境同时设 production，并在 Admin 文件中配置真实通知 Provider。
- 两个补丁内的版本、镜像名和容器名必须符合现场；镜像先导入实际调度节点的 CRI，不能只导入运维机 Docker。
- 两个补丁的 `envFrom` 会替换现场该列表；若有额外必要引用，将其一并保留。其他未知 env、resources、插件卷、hostNetwork、RBAC、OTel 等仍保留；新增接管字段时同步清除同名显式旧 env。
- 旧 Nginx 挂载删除项须与现场一致；若其他容器仍引用该卷，不能删除其 volume。外部 Wasm/原生动态库挂载继续保留，旧 Wasm 绑定需先核对制品。

每行 `KEY=value`、JSON 单行、不加 shell 外层引号、不 source；包含凭据的填写副本和生成 YAML 通过私有渠道交付。

## 2. 从两份文件整体生成 K8s 配置

只在本地生成，不修改集群。每份 env 整体存入该服务的一个 K8s Secret；普通参数和凭据统一加载，不需要运维再拆字段。配置对象各自创建在对应服务 namespace。

```bash
umask 077
mkdir -m 700 "$RELEASE_DIR/generated"
kubectl -n "$NS" create secret generic "spacegate-admin-config-$VERSION" --from-env-file="$RELEASE_DIR/config/spacegate-admin.env" --dry-run=client -o yaml > "$RELEASE_DIR/generated/admin-config.yaml"
kubectl -n "$GATEWAY_NS" create secret generic "spacegate-config-$VERSION" --from-env-file="$RELEASE_DIR/config/spacegate.env" --dry-run=client -o yaml > "$RELEASE_DIR/generated/gateway-config.yaml"
```

任何命令报错先停止。已生成过时先核对文件，不重复运行覆盖已审定配置。版本应与补丁引用一致；后续仅换镜像可继续引用已验收的配置版本。

## 3. 备份和审查补丁差异

先按 [详细发布手册](../../../docs/k8s/native-queue-release-runbook.md) 备份工作负载和引用对象，记录回滚 revision。停止新增模型流量并确认 queued/executing/Body 清零后再更新网关。

以下读取现场并在本地合并，不修改集群：

```bash
kubectl -n "$NS" get deployment "$ADMIN_DEPLOYMENT" -o yaml > "$RELEASE_DIR/admin.before.yaml"
kubectl -n "$GATEWAY_NS" get daemonset "$GATEWAY_DAEMONSET" -o yaml > "$RELEASE_DIR/gateway.before.yaml"
kubectl patch --local -f "$RELEASE_DIR/admin.before.yaml" --type=strategic --patch-file "$RELEASE_DIR/k8s/admin.patch.yaml" -o yaml > "$RELEASE_DIR/admin.after.yaml"
kubectl patch --local -f "$RELEASE_DIR/gateway.before.yaml" --type=strategic --patch-file "$RELEASE_DIR/k8s/gateway.patch.yaml" -o yaml > "$RELEASE_DIR/gateway.after.yaml"
diff -u "$RELEASE_DIR/admin.before.yaml" "$RELEASE_DIR/admin.after.yaml"
diff -u "$RELEASE_DIR/gateway.before.yaml" "$RELEASE_DIR/gateway.after.yaml"
```

`diff` 返回 1 表示有差异。重点审查镜像、单个 envFrom 配置引用、旧显式 env 删除、旧 Nginx 挂载清理和其他现场设置保留。不把 after.yaml 当成本包发布入口。

## 4. 发布与验收

先执行 API server dry-run；它只检查资源可被接受，不检查数据库可达性或模型功能：

```bash
kubectl apply --dry-run=server -f "$RELEASE_DIR/generated/"
kubectl -n "$NS" patch deployment "$ADMIN_DEPLOYMENT" --type=strategic --patch-file "$RELEASE_DIR/k8s/admin.patch.yaml" --dry-run=server
kubectl -n "$GATEWAY_NS" patch daemonset "$GATEWAY_DAEMONSET" --type=strategic --patch-file "$RELEASE_DIR/k8s/gateway.patch.yaml" --dry-run=server
```

确认后应用配置和本次需要更新的工作负载：

```bash
kubectl apply -f "$RELEASE_DIR/generated/"
kubectl -n "$GATEWAY_NS" patch daemonset "$GATEWAY_DAEMONSET" --type=strategic --patch-file "$RELEASE_DIR/k8s/gateway.patch.yaml"
kubectl -n "$GATEWAY_NS" rollout status daemonset/"$GATEWAY_DAEMONSET" --timeout=600s
kubectl -n "$NS" patch deployment "$ADMIN_DEPLOYMENT" --type=strategic --patch-file "$RELEASE_DIR/k8s/admin.patch.yaml"
kubectl -n "$NS" rollout status deployment/"$ADMIN_DEPLOYMENT" --timeout=600s
```

只更新 Admin 时省略网关补丁，并保持 token 与现有网关相同。修改配置需要重建 Pod 才生效；版本化引用变更会触发常规 rollout，DaemonSet 的 OnDelete 策略则按现场流程逐节点更新。保留旧配置对象与发布前 revision 供回滚，不删除数据库、Redis 或 S3。

Service 保留 9080，Ingress 保留 `/api/v1/...`，不直接暴露内部 Rust:9081。受限 non-root/只读根部署需单独核验合并入口的 root 目录准备/gosu 行为。

发布后按详细手册验收登录/刷新、插件发现/配置读写、逐副本队列状态、受控延时请求、取消/超时、S3 回读清理和访问审计。Pod Ready 不代表业务验收。本包不创建 namespace、CRD、GatewayClass、RBAC 或完整网关 DaemonSet；尚未在目标 K8s 集群实际发布。
