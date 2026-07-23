# Spacegate 代码审核报告

> 文档生成日期：2026-05-12  
> 范围：整个 Spacegate workspace（`spacegate-kernel` / `spacegate-plugin` / `spacegate-model` / `spacegate-config` / `spacegate-shell` / extensions / binaries / SDK）

本文档对仓库整体架构、各 crate 功能点、亮点及风险进行归纳，便于评审与后续修复跟踪。

---

## 一、项目总览

Spacegate 是基于 Rust 与 hyper 的 **库优先（library-first）** API 网关，强调云原生（Kubernetes Gateway API）与插件扩展。`Cargo.toml` 中 workspace 成员包含：

- **二进制**：`binary/spacegate`、`binary/admin-server`
- **核心库**：`crates/kernel`、`crates/plugin`、`crates/model`、`crates/config`、`crates/shell`
- **扩展**：`crates/extension/axum`、`crates/extension/redis`
- **示例**：`examples/sayhello`、`examples/socks5-proxy`、`examples/mitm-proxy` 等

分层关系（自下而上）：

| 层 | crate | 职责 |
|----|--------|------|
| 数据模型 | `spacegate-model` | 网关/路由/插件/匹配规则 DTO，可选 ts-rs 导出 |
| 配置后端 | `spacegate-config` | 文件 / K8s / Redis / 内存：CRUD + 事件监听 |
| 核心运行时 | `spacegate-kernel` | TCP 监听、HTTPS、路由匹配、Backend、helper layer |
| 扩展库 | `spacegate-ext-axum`、`spacegate-ext-redis` | 全局 axum 服务、Redis 客户端仓库 |
| 插件系统 | `spacegate-plugin` | Plugin trait、动态库、内置插件、挂载点 |
| 集成入口 | `spacegate-shell` | 配置与内核映射、热更新、生命周期 |
| 二进制 | `spacegate`、`spacegate-admin-server` | 网关进程、管理后台 |
| SDK | `sdk/admin-client` | TypeScript，对接 admin-server |

---

## 二、`spacegate-kernel`

### 2.1 主要功能

1. **TCP 监听与协议嗅探**：`SgListen` 通过 `peek` 后由 `TcpService::sniff` 选择 HTTP/HTTPS/SOCKS5 等。
2. **HTTP/1.1、HTTP/2、WebSocket、HTTPS**：`Http`/`Https` 实现 `TcpService`；`HyperServiceAdapter` 将请求转为 `SgBody` 并注入 `PeerAddr`、`EnterTime`、`Reflect`。
3. **网关装配**：`http_gateway::Gateway`（builder）含网关级插件链、`HttpRoute` 表、`Reloader` 支持热更新路由。
4. **主机名匹配**：`HostnameTree`（`match_hostname.rs`），支持 IPv4/IPv6、通配域名、优先级排序。
5. **路由匹配**：`HttpRouteMatch` 支持 path（Exact/Prefix/Regex）、headers、query、method；多重 match 在单条规则内为 AND；`Vec` 层为 OR。
6. **后端**：`http_backend_service`（`x-forwarded-for`、WebSocket 升级与双向拷贝）、`static_file_service`、全局 `ClientRepo` 与可插拔 `HttpClient`。
7. **辅助层**：`TimeoutLayer`、`ReloadLayer`（`ShardedLock`）、`Balancer`（`IpHash` / 加权随机）、`MapRequest`/`MapFuture`、`RouterService`。
8. **扩展与工具**：`Reflect`、`Defer`、`OriginalIpAddr`、`MatchedSgRouter`、`Authorization<Basic/Bearer>`、`SgBody`（dump 后可克隆）。

### 2.2 亮点

- `BoxLayer` 与 tower 组合良好，网关/路由/规则/后端多级挂载清晰。
- `Reloader` + `OnceLock` 读多写少场景友好。
- `HostnameTree` 设计文档与测试较完整。

### 2.3 风险与问题

- **TLS 客户端默认跳过服务端证书校验**：`ClientRepo::default` 使用 `get_rustls_config_dangerous`；`SgParameters::ignore_tls_verification` 在代码中未见实质接线。**生产环境风险高**，建议默认走系统根证书，仅显式配置才关闭校验。
- **静态文件路径规范化**：`canonicalize` 失败时回退到 `dir`，可能削弱「必须在目录下」的语义，建议失败即 404。
- **`HttpBackendService` 使用 `unwrap_unchecked`**：可改为显式处理以更清晰。
- **`create_http_router` 中 hostname 索引**：当 `route.hostnames` 非空时，新建节点误落到 `"*"` 的逻辑需核对是否为 bug（应绑定具体 hostname）。
- **`SgBody::clone` 未 dump 会 panic**：插件作者易踩坑，需在文档中突出。
- **方法匹配注释与 `Vec` 的 OR 语义**：注释若写「仅当指定 method」易与实现不一致。

---

## 三、`spacegate-plugin`

### 3.1 主要功能

1. **Plugin trait**：`CODE`、`call`、`create`，可选 `MONO`、`schema_opt`、元数据。
2. **`PluginRepository`**：全局注册表、实例 CRUD、`register_dylib`、快照与挂载追踪。
3. **`PluginInstance`**：`ArcSwap` 热替换函数、生命周期钩子、`DropTracer` 防悬挂挂载索引。
4. **挂载点**：网关 / 路由 / 规则 / 后端四级 `MountPointIndex`。
5. **内置插件（按 feature）**：如 `static-resource`、`limit`（Redis Lua）、`header-modifier`、`redirect`、`rewrite`、`set-version`、`set-scheme`、`maintenance`、`inject`、`east-west-traffic-white-list`，以及 Redis 系列（`redis-count`、`redis-limit`、`redis-time-range`、`redis-dynamic-route`）。

### 3.2 亮点

- `PluginError` 统一错误响应与 `X-Plugin-Error` 头。
- 部分 Redis 插件含 testcontainers 集成测试。

### 3.3 风险与问题

- **`redirect` 插件**：解析 URL 后未真正返回 3xx 或未改写请求，接近 no-op，需补全实现。
- **遗留/禁用模块**：`breaker.rs` 空文件；`decompression`/`status`/`retry` 等与旧 API 耦合且未在 `register_prelude` 启用，建议清理或重写。
- **`SystemTime::now().duration_since(UNIX_EPOCH).expect(...)`**：时间异常时可能 panic。
- **仓库锁**：`RwLock` + 多层 `expect`；钩子里若再次操作仓库可能死锁，需在文档约束。
- **`reflect` 扩展缺失会 panic**：非标准入口构造的请求需注意。
- **`FromBackend::unsafe new` 用法**：可与实际调用路径再核对是否必须 unsafe。

---

## 四、`spacegate-model`

### 4.1 主要功能

- `SgGateway`、`SgHttpRoute`、`SgBackendRef`、`BackendHost`、`PluginInstanceId/Name`、`PluginConfig`、`PluginInstanceMap`。
- 可选 `typegen`（ts-rs）供前端/SDK。
- K8s 相关扩展（CRD 等）。

### 4.2 风险与问题

- **`PluginInstanceName` 的 `Display` 与 `FromStr` 不一致**：Mono 显示为 `m` 而解析期望 `g`，可能影响依赖字符串往返的配置/通道。
- **`PluginInstanceMap` 反序列化**：错误路径使用 `eprintln!`，建议改为 `tracing`。

---

## 五、`spacegate-config`

### 5.1 主要功能

- Trait：`Create`、`Retrieve`、`Update`、`Delete`；`CreateListener` + `Listen`；`ConfigType` / `ConfigEventType`。
- **实现**：`Memory`（静态）、`Fs`（目录布局 + Unix SIGHUP / Windows notify）、`K8s`（多资源 watch + SIGHUP 全局重载）、`Redis`（hash + pubsub）。
- **Discovery**：实例列表与可选后端发现（如 fs 下读 `/var/www`）。

### 5.2 风险与问题

- **K8s 路由事件**：`process_http_spaceroute_event` 中 Applied 与 Delete 的事件类型是否应区分 Update/Delete，需与 shell 中「全量拉路由」行为对照，避免语义混淆。
- **监听任务中 `send(...).expect`**：通道关闭会导致 panic。
- **`Fs::modify_cached` 全目录删建**：中断可能丢配置，宜加备份或原子写。
- **`redis/listen.rs` 中未使用的 `CHANGE_CACHE`**：死代码。
- **`RedisListener::CONFIG_LISTENER_NAME` 误写为 `"file"`**：应为 `"redis"` 以免日志误导。

---

## 六、`spacegate-shell`

### 6.1 主要功能

- `startup_file` / `startup_k8s` / `startup_redis` / `startup_static` → 统一 `startup`。
- `RunningSgGateway`：`global_init`、`global_reset`、`global_update`（`Reloader` 热更路由）。
- 配置到内核：`collect_http_route`、`global_batch_mount_plugin`、K8s Service 扩展注入。
- 启用 `ext-axum` 时：健康检查、`/control/push_event`、静态页等。

### 6.2 风险与问题

- **Route 类事件**：handler 对 Create/Update/Delete 一律 `retrieve_config_item_all_routes` 后整体更新，语义依赖「全量正确」；与 K8s 事件类型需一起审视。
- **插件初始化失败**：当前多为日志后继续，可按策略支持 fail-fast。
- **全局 `Mutex` 中毒**：`expect("poisoned lock")` 后难以恢复。
- **TLS `enable_secret_extraction`**：生产宜可配置关闭。

---

## 七、扩展库

### `spacegate-ext-redis`

- `RedisClient::get_conn` / `From<&str>` 使用 `unwrap`/`expect`，配置错误易 panic；建议提供 `try_*` API。

### `spacegate-ext-axum`

- `GlobalAxumServer` 关停路径存在 `expect`；`InternalError` 里有 `unwrap` 组 Response。

---

## 八、`binary/spacegate`

- Clap 参数：`file:`/`k8s:`/`redis:`/`static:`；可选动态库目录扫描加载。
- 缺 feature 时 dylib 仅 `eprintln`，建议统一 tracing。

---

## 九、`binary/admin-server`

### 主要功能

- `/config/*`、`/plugin/*`、`/auth/login`、`/discovery/*`。
- JWT + 可选 SK 摘要；`X-Client-Version` / `X-Server-Version` 乐观并发。
- 发现：实例健康、插件列表/schema 缓存、向网关 `push_event` 触发重载。

### 风险与问题

- **空文件**：`mw/instance_select.rs` 等遗留。
- **跨平台**：`clap` 等处 `unix` 专有 import/默认值未守卫时 Windows 编译可能失败。
- **健康检查与 `sync_attr_cache` 缓存**：若使用 `Instant::elapsed() >= Duration::ZERO` 判断是否过期，逻辑恒为「已过期」，缓存失效——应改为与 `Instant::now()` 比较。
- **依赖版本**：如 `tower-http` 与 workspace 不一致可能导致重复编译。
- **未配置鉴权时中间件放行**：部署文档需强调必须配置密钥。

---

## 十、`sdk/admin-client`

- Axios 封装，与 admin-server API 对齐；版本冲突与 401 自定义异常。
- 注意全局 client 与 `clientVersion` 刷新页丢失导致的首次 409。

---

## 十一、示例

- **sayhello**：动态库插件最小示例。
- **socks5-proxy**：`TcpService` + 端口多协议嗅探。
- **mitm-proxy**：CONNECT + 动态证书 MITM 演示。

---

## 十二、横向问题汇总

| 优先级 | 问题 |
|--------|------|
| 高 | TLS 默认信任任意后端证书；`ignore_tls_verification` 未接线 |
| 高 | `redirect` 插件未真正重定向 |
| 高 | `GatewayRouter` hostname 索引与通配 `*` 的逻辑需复核 |
| 高 | K8s 监听中路由事件类型与 shell 全量更新语义 |
| 高 | `PluginInstanceName` Display/FromStr 不一致 |
| 高 | admin-server 健康/attr 缓存时间判断错误 |
| 中 | 大量 `unwrap`/`expect`；Redis/Axum 扩展 panic 路径 |
| 中 | Windows 编译（unix-only 模块） |
| 中 | 死代码与误填常量（如 Redis listener 名称） |
| 低 | tracing 替代 eprintln；依赖版本对齐 |

---

## 十三、建议修复顺序（供迭代跟踪）

1. **安全与正确性**：TLS 默认策略、`redirect`、hostname 路由索引、K8s 事件语义、PluginInstanceName 往返、admin-server 缓存判断。
2. **健壮性**：减少 expect；Redis `try_get`；插件钩子使用规范文档。
3. **可维护性**：清理 breaker/status 等废弃路径；统一 tower-http 版本；修正 `CONFIG_LISTENER_NAME`。

---

## 十四、修订历史

| 日期 | 说明 |
|------|------|
| 2026-05-12 | 初版：基于全仓库结构与关键源码路径的审核汇总 |
