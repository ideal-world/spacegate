# 外部原生插件交付与安装

本文面向 SpaceGate 发布方、第三方插件作者和部署管理员。当前原生插件通过 `extern "Rust" register(&PluginRepository)` 加载。支持的交付单位是**面向某一网关镜像 digest 构建和验收的受信插件包**。换网关镜像后，需要针对新镜像重新验收。

当前网关可以在启动时扫描 `.so`，Admin 可以发现已注册的插件类型并管理实例和绑定。发布配套 SDK、安装前兼容性校验和 Admin 原生包上传目前没有产品入口。下面分别说明现在可执行的部署流程和对外发布所需的配套产物。

## 1. 插件作者如何打包

### 获取目标网关的构建基线

发布方需随每个支持外部原生插件的发行版提供一份构建基线，至少包含：

| 输入 | 需要固定的内容 |
| --- | --- |
| 网关身份 | 镜像 digest、目标平台、实际运行程序，例如 HAI 网关或原版 `spacegate` |
| 源码与依赖 | SpaceGate 和宿主应用的构建提交、实际应用的 Cargo patch、构建宿主最终使用的 `Cargo.lock`、启用的 Cargo features |
| 构建工具 | Builder 镜像 digest、`rustc -vV`、目标三元组、链接工具、release profile 和 `RUSTFLAGS` |

HAI 镜像把 SpaceGate 作为本地 patch 编入宿主应用。它的最终依赖解析和 features 应以 **HAI 宿主构建**为准。只拿 SpaceGate Git tag、crates.io 版本号或初始复制进去的锁文件，无法重建这份基线。缺少基线时，由发布方在原发布构建环境代打包，或先补齐发行版构建资料。 [HAI 镜像构建](../../resource/docker/spacegate-k8s/Dockerfile)

### 编译并生成制品

在发布方提供的构建环境里添加插件 crate。依赖必须使用该环境提供的 SpaceGate 源码和 feature 配置，并启用宿主使用的 `dylib` 接口。实现 `Plugin`，给 `Plugin::CODE` 分配唯一标识，例如 `vendor-rate-limit`，并通过 `dynamic_lib!` 导出 `register`。如需 Admin 生成表单，在与宿主匹配的 `schema` feature 下实现 `Plugin::schema_opt()`。

```toml
[lib]
crate-type = ["cdylib"]
```

```rust
use spacegate_plugin::dynamic_lib;

dynamic_lib! { VendorRateLimit }
```

用发行版的工具链、target 和 release profile 执行构建。以下变量由该发行版构建环境提供，`PLUGIN_CRATE` 填插件 crate 的 Cargo package 名：

```bash
cargo build --locked --release --target "$TARGET" -p "$PLUGIN_CRATE"
```

新增插件 crate 或依赖时，发布方需更新插件构建 workspace 的锁文件，并对照宿主的解析结果和启用的 features 检查共享依赖。锁文件只能固定依赖解析，feature 仍由参与构建的 crate 决定。例如 HAI 宿主启用了 `serde_json/preserve_order`，漏掉该特性可能改变跨库传递的数据布局。最终在目标镜像上执行第 2 节验收。

建议交付一个带版本的压缩包，并附带以下内容。这里的清单供发布与部署工具校验，当前网关启动扫描只读取其中单独取出的 `.so`。

```text
vendor-rate-limit-1.2.0-linux-amd64.tar.gz
  libvendor_rate_limit.so
  manifest.json
  SHA256SUMS
  README.md
  config.example.json
```

`manifest.json` 应记录 `plugin_code`、插件版本、`.so` 文件名及 SHA-256、目标平台、已验收的网关镜像 digest、构建基线标识和验收记录。说明文档给出配置字段、绑定范围、预期行为及验收请求。完整包经发布方可信渠道交付。

## 2. 部署前如何确认兼容

在隔离测试环境中，用**与拟部署实例完全相同的镜像 digest 和平台**逐项验收。原生代码在网关进程内执行，ABI 不匹配可能导致误注册或进程崩溃。任一项失败都应阻止安装。

| 阶段 | 验收标准 |
| --- | --- |
| 制品校验 | 发布来源可信、哈希匹配、平台与 digest 和制品清单一致。确认 ELF 架构、动态依赖、导出的 `register` |
| 注册 | 目标网关可启动，运行时 `GET /plugin-list` 包含精确的插件 `code`，且进程持续可用 |
| 实例初始化 | 创建有效配置后，`GET /plugin-snapshot/repo` 包含对应实例。用插件无效配置或依赖故障验证其承诺的错误处理 |
| 绑定与请求 | 绑定到测试流量，按插件说明执行请求并校验结果。健康检查和 Admin 列表只用于辅助诊断 |

静态检查可在具备 GNU binutils 的构建环境执行：

```bash
sha256sum -c SHA256SUMS
file libvendor_rate_limit.so
readelf --dyn-syms --wide libvendor_rate_limit.so
readelf -d libvendor_rate_limit.so
```

在目标镜像里核对动态依赖可解析，再启动隔离网关。以下变量指向**测试网关**的管理地址、插件 code 和实例名。示例命名实例的快照键为 `n-<name>`：

```bash
curl -fsS "$GATEWAY_API/plugin-list" |
  jq -e --arg code "$PLUGIN_CODE" 'any(.[]; .code == $code)'

curl -fsS "$GATEWAY_API/plugin-snapshot/repo" |
  jq -e --arg code "$PLUGIN_CODE" --arg name "n-$INSTANCE_NAME" \
    '.[$code].instances[$name] != null'
```

实际绑定以测试请求结果为准。至少对正常请求和插件应拦截或改写的请求分别检查状态、响应头或业务结果。测试带有鉴权、限流等能力的插件时，还要确认失败时网关的处理符合业务预期。只有 ELF 可加载、`register` 符号存在，或端口仍监听，都不足以判断插件可用。 [网关运行时接口](../../crates/plugin/src/ext/axum.rs)

## 3. 管理员如何安装

### 先部署插件二进制

安装经过验收的包，并将需要启用的 `.so` 从包中取出，放进版本化、只读挂载的外部插件目录。所有承接同一配置的网关实例应使用同一份插件制品。扫描时机是**网关进程启动**。

| 运行方式 | 扫描配置 |
| --- | --- |
| HAI all-in-one | 将制品目录挂载到 `/var/lib/spacegate/plugins`。启动脚本把 `EXTERNAL_NATIVE_PLUGIN_DIR`（默认该目录）传入 `PLUGIN_DIR` |
| HAI K8s 镜像 | 在网关容器设置 `PLUGIN_DIR=/var/lib/spacegate/plugins`，挂载到同一路径。变量值为单个目录 |
| 原版 SpaceGate | 启用 `dylib`，使用 `--plugins` 或 `PLUGINS` 指定目录。支持逗号分隔 |

all-in-one 的 Compose 服务中追加挂载后重建该服务容器：

```yaml
volumes:
  - ./native-plugins/<网关插件集合版本>:/var/lib/spacegate/plugins:ro
```

K8s 可由 InitContainer 校验制品并写入共享 `emptyDir`，也可使用启动前已填充的受控卷。下面是基于已有 `external-plugins` 卷的 HAI DaemonSet Pod 模板片段。制品镜像须使用已验收的 digest，并在 `/payload` 中提供 `.so` 与 `SHA256SUMS`：

```yaml
initContainers:
  - name: install-native-plugin
    image: "registry.example.com/vendor/rate-limit@sha256:<已验收的制品镜像摘要>"
    workingDir: /payload
    command: ["/bin/sh", "-ec"]
    args:
      - |
        sha256sum -c SHA256SUMS
        cp libvendor_rate_limit.so /staging/
    volumeMounts:
      - name: external-plugins
        mountPath: /staging
containers:
  - name: spacegate
    env:
      - name: PLUGIN_DIR
        value: /var/lib/spacegate/plugins
    volumeMounts:
      - name: external-plugins
        mountPath: /var/lib/spacegate/plugins
        readOnly: true
```

将片段合并到现有 Pod 模板并保留网关的镜像、配置、探针和其他挂载。部署到所有目标实例后，逐个核对 `/plugin-list`。基础 DaemonSet 里的 `PLUGINS` 对应原版加载器。 [K8s 安装](../k8s/installation.md)

### 再创建实例和绑定

1. **Admin** 通过插件目录刷新读取已注册的类型，随后使用该 `code` 创建实例、录入配置并绑定到目标网关或路由。Admin 现有目录缓存从发现到的首个网关实例读取，列表出现插件不代表整个集群已安装。刷新目录本身不会加载 `.so`。 [Admin 插件目录实现](../../../spacegate-admin/rust/admin-server/src/service/plugin.rs)
2. **文件配置** 可创建 `plugin/vendor-rate-limit.primary.json`，写入插件的配置 JSON。按实际绑定层级追加 `{"code":"vendor-rate-limit","kind":"named","name":"primary"}`。 [文件配置结构](DEVELOPMENT.md#54-文件系统配置格式示例)
3. **K8s 配置** 可通过 `SgFilter.spec.filters` 写入 `code`、`name`、`config`，通过同一个资源的 `spec.targetRefs` 关联目标。配置里的 `code` 使用 `Plugin::CODE`。 [SgFilter CRD](../../resource/kube-manifests/spacegate-gateway.yaml)

完成后对每个接流量的实例执行运行时及请求验收。升级时先在新镜像和新插件包的组合上验收，再按网关发布流程切换流量。回滚时恢复上一组已验收的网关镜像、插件制品和绑定配置，并重启对应实例。卸载时先解除绑定、移除实例，再移除制品并重启网关。

## 对外发布的范围

现有能力可交付给**有部署权限、使用受信插件、按发行版验收**的客户。对这些客户，正式发布需要配套交付构建基线或 SDK、制品校验规则和可复用的目标镜像验收步骤。若产品范围包括客户从 Admin 自助上传、安装、升级和回滚，还需要实现安装前校验、制品分发、逐副本运行状态以及对应的管理入口。若还要承诺插件跨网关版本兼容，则需要版本化且可维护的插件 ABI。当前加载器直接接收 Rust `PluginRepository`，没有这一兼容契约。 [动态加载实现](../../crates/plugin/src/dynamic.rs)
