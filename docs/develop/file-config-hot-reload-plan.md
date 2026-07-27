# File 配置热更新实施计划

## 目标

让 Linux file 配置后端在管理端保存插件实例、插件绑定、路由或网关配置后自动更新运行时配置；保留 `SIGHUP` 作为人工全量重载入口。K8s 与 Redis 配置后端不在本计划的改动范围内。

## 第一阶段：去抖后的全量刷新

### 范围

- 修改 `crates/config/src/service/fs/listen.rs` 的 Unix 实现。
- 递归监听 `Fs.dir`，接收配置目录中的文件创建、修改、删除或重命名事件。
- 将连续事件以 300ms 去抖为一个 `ConfigType::Global + Update` 事件。
- 同时监听 `SIGHUP`；收到信号时立即产生同一个全量事件，不等待去抖计时器。
- 继续复用 `spacegate-shell` 已有的 `Global` 处理：停止旧网关、重新读取完整配置、重建插件实例和网关。

### 事件语义

1. Admin-server 写入一个或多个 `/etc/spacegate` 文件。
2. watcher 只记录“目录已变更”，不根据单个临时文件读取配置。
3. 300ms 内的新事件会重置计时器，等待整批文件写入结束。
4. 计时器结束后发送一次全量更新，读取最终一致的配置快照。
5. `SIGHUP` 直接发送全量更新，仍作为 bind mount、调试和运维兜底方式。

### 验证

- 在 Unix 上写入配置目录文件后，listener 在去抖窗口结束时产生一个 `Global/Update` 事件。
- 未写入文件且未发送 HUP 时不会产生事件。
- `cargo test -p spacegate-config` 通过。
- all-in-one 回归：创建 `hai-auth` 实例并绑定 `/api/` 路由后，不发送 HUP 直接请求 `GET /api/v1/model/111`，返回 `401` 与 `missing_api_key`。

## 第二阶段：精细热更新

### 目标

避免无关配置变更触发全网关重建，特别是防止 MCP SSE/Streamable HTTP 长连接被与其无关的保存操作中断。

### 设计

- 文件后端的精细 gateway/route CRUD 必须定点写入对应文件；不能继续调用 `modify_cached()`，因为它会先清空整个配置目录，导致一次 route 保存同时修改根 `config.json` 并退化为全量刷新。
- listener 将稳定文件路径映射为精细事件：
  - `plugin/<id>.json` → `ConfigType::Plugin`；
  - `gateway/<gateway>/route/<route>.json` → `ConfigType::Route`；
  - `gateway/<gateway>/config.json` → `ConfigType::Gateway`；
  - `config.json` 或无法归类的批量变更 → `ConfigType::Global`。
- 同一批文件变更必须按依赖顺序合并：先更新插件实例，再更新路由，最后处理网关；无法保证完整批次时退回全量事件。
- 路由更新继续使用 `RunningSgGateway::global_update` 的 reloader；插件更新继续使用 `PluginRepository::create_or_update_instance`。
- 单个插件实例删除暂时直接退回全量刷新；只有在 shell 层先重载所有引用路由、再移除实例后，才可以把它改为精细删除，避免短暂的“missing instance”日志和空插件链。
- 目录删除、rename、watch overflow 和无法分类的路径同样退回全量刷新；稳定的单文件 create/update/delete 才进入精细事件队列。
- 保留全量事件用于 HUP、批量导入、目录替换、watch overflow、解析失败后的下一次变更恢复。

### 第二阶段验收

- 修改单个插件配置只更新该实例，不重建无关网关。
- 修改单条 HTTPRoute/MCPRoute 只替换该网关的 router service。
- 在另一条路由保存或插件实例更新时，已有 MCP SSE 流保持连接。
- 批量保存的插件与路由不会发生路由先挂载、实例后创建的竞态。
- 删除插件实例时退回全量刷新，优先保证最终路由和实例快照一致。
- K8s 与 Redis 的现有事件语义和测试不发生变化。

## 非目标

- 不支持运行时发现、加载或卸载新的 native `.so` 插件代码。
- 不以文件监听作为 Wasm 二进制版本更新机制。
- 不修改 K8s CRD/watch、Redis PubSub 或 Admin API 的公共接口。
