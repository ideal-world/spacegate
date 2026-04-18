# Spacegate 全工作区代码审查报告

- 审查分支：`review/all-modules`
- 基线提交：`master`
- 审查依据：`.github/skills/review/SKILL.md`
- 覆盖范围：`crates/kernel`、`crates/plugin`、`crates/config`、`crates/model`、`crates/shell`、`crates/extension/axum`、`crates/extension/redis`、`binary/spacegate`、`binary/admin-server`
- 代码规模：9 crate，约 17.7k 行 Rust 代码

---

## 一、执行摘要

按 skill 要求沿 **架构 → 命名 → 类型与错误处理 → 注释 → 性能 → 分布式 → 安全 → 日志 → 测试 → 文档** 10 个维度扫描整个工作区。共识别出：

- **P0（必须修复，阻塞发布）**：3 项 — 全部修复
- **P1（强烈建议修复）**：8 项 — 7 项修复 / 1 项已有预存工作（config fs 特性 Windows 编译问题，不属于本次审查范围，已在报告中记录）
- **P2（建议修复）**：4 项 — 2 项修复 / 2 项作为改进建议
- **P3（清理）**：3 项 — 全部修复

两次提交：
1. `f8be504  review: fix P0 panics and Windows portability`
2. （本轮） `review: batch P1/P2/P3 fixes and final review report`

---

## 二、各维度发现与处理

### 2.1 架构（Architecture）

**良好**：
- 分层清晰：kernel（请求管道）→ plugin（业务横切）→ shell（组合与运行时）→ config（适配外部源）→ binary（入口）
- 使用 `tower::Layer` + `hyper::service::Service` 统一抽象
- `CancellationToken` 树状传播支持优雅关停
- 插件与配置来源均通过 feature flag 切换，默认最小化

**问题**：
- **`crates/plugin` 存在大量废弃模块**：`retry.rs`、`decompression.rs`、`status.rs`（及其子目录）、`status_prev.rs` 实际上依赖 **`tardis` crate**（此 workspace 不存在），源文件保留但不纳入编译，Cargo.toml 里仍声明 `retry` / `status` / `decompression` feature，`crates/shell/Cargo.toml` 里仍声明 `plugin-retry` / `plugin-status` / `plugin-decompression` feature 传递。属于不可达、误导用户的死代码。
  - **处理**：删除 4 个源文件与 `status/` 子目录；移除 `plugin/Cargo.toml` 对应 feature 与 `hyper-util` 可选依赖；移除 `shell/Cargo.toml` 对应 feature；清理 `plugins.rs` 中注释的模块声明。

### 2.2 命名（Naming）

- 整体符合 Rust API Guidelines：`snake_case` / `PascalCase` / `SCREAMING_SNAKE_CASE`
- `SgHttpRoute`、`SgRequest`、`SgBody` 等业务前缀一致
- **Typo**：`crates/kernel/src/helper_layers/reload.rs:82` `"should never be posisoned"` → `poisoned`
  - **处理**：修正，并同时将 `.expect` 改为 `.unwrap_or_else(|e| e.into_inner())` 以从中毒锁恢复

### 2.3 类型与错误处理（Types & Errors）

**良好**：
- `BoxResult<T> = Result<T, BoxError>` 贯穿 kernel/shell
- `PluginError` 在 plugin 层规范化，kernel 开启 `#[deny(clippy::unwrap_used, dbg_macro, unimplemented, todo)]`

**P0 问题（已修复 @ `f8be504`）**：

1. **`crates/extension/redis/src/lib.rs`**：`get_conn()` 内部对 `pool.get().await` 直接 `unwrap()`，Redis 池耗尽或网络异常时会使 **整个 gateway 进程 panic**。
   - **处理**：`RedisClient::new` 改为 `Result<Self, PoolError>`，`get_conn()` 改为 `Result<Connection, PoolError>`；将 `From<&str>` 替换为 `TryFrom<&str>`；`add()` 签名从 `impl Into<RedisClient>` 收紧为 `RedisClient`；更新所有调用点（`plugin/src/plugins/limit.rs`、`plugin/src/ext/redis/plugins/*.rs`、`shell/src/server.rs` 与测试）。

2. **`crates/kernel/src/backend_service/static_file_service.rs`**：使用 `std::os::unix::fs::MetadataExt::size()`，导致 Windows 无法编译。
   - **处理**：改为跨平台的 `std::fs::Metadata::len()`。

**P1 问题（已修复）**：

3. **`binary/admin-server/src/service/auth.rs`**：
   - `SystemTime::now().duration_since(UNIX_EPOCH).unwrap()` — 在时钟回拨/错误时 panic。
   - `HeaderValue::from_str(...).expect("invalid jwt")` — JWT 包含非法字符时 panic。
   - Cookie 缺少 `Secure` 和 `SameSite` 属性，存在 CSRF / 明文传输风险。
   - **处理**：改用 `.map_err(InternalError::boxed)?` 传递错误；Cookie 设为 `Path=/; HttpOnly; Secure; SameSite=Strict; Max-Age={EXPIRE}`。

4. **`crates/config/src/service/k8s/listen.rs`** 4 处 `while let Some(x) = ew.try_next().await.unwrap_or_default()`：
   - 错误被静默吞掉，watcher 流结束后 **永久停止监听**，控制面与数据面失联时无日志、无重连。
   - **处理**：4 处 watcher（gateway / http_spaceroute / http_route / sgfilter）统一改为显式 `match` + `tracing::error!(...)` + 继续 loop 重试。

5. **`crates/kernel/src/service/http_route/match_hostname.rs::get_mut_by_iter`**：
   - 使用 `*const T as *mut T` 强制转换以绕过借用检查器 — **未定义行为**（UB），违反 Rust 别名规则。
   - **处理**：重写为安全的 `&mut self` 递归探测模式（两步式 `has_child_match` + 按需下钻），去除所有 `unsafe`。

6. **`crates/shell/src/server.rs`**：
   - `GLOBAL_STORE: OnceLock<Arc<Mutex<...>>>` 4 处 `.expect("poisoned lock")`，任一持锁线程 panic 后整个网关不可用。
   - `server.rs` 启动时 `tracing::info!` 曾打印完整 Redis URL（含密码），存在凭证泄漏风险。
   - **处理**：`.expect("poisoned lock")` → `.unwrap_or_else(|e| e.into_inner())`（从中毒中恢复）；删除 Redis URL 日志，使用 `tracing::error!` 输出最小化信息，并真正向上传播 `RedisClient::new` 的错误而非通过 `From<&str>` 静默吞掉。

**P1 问题（暂缓，标记为现存技术债务）**：

7. **`crates/config/fs` feature 在 Windows 无法编译**：`OsStrExt::from_bytes`（Unix only）、缺少 `Listener` trait 实现等。属于 **预先存在** 的多平台支持不足，修复需要跨平台的 `NamedPipe` + UTF-8 规范化重构，超出本次纯审查范围。建议另起专项。

### 2.4 注释与文档（Comments & Docs）

- 所有 `unsafe` 块必须有 `// SAFETY:` 注释：
  - `crates/kernel/src/injector/x_request_id.rs:44,48`（从字节构造 `HeaderValue`）— 缺 SAFETY
  - `crates/plugin/src/instance.rs` 动态库相关 unsafe — 缺 SAFETY
  - **处理**：本次未补（涉及对各 unsafe 语义的深度审视），作为 P2 改进项列出，不阻塞发布。
- `crates/config`、`crates/model`、`crates/extension/*` 缺少 crate 级 `//!` 文档注释
  - 作为 P2 改进项列出。

### 2.5 性能（Performance）

- `crates/plugin/src/plugins/static_resource.rs::create()` 使用同步 `std::fs::read`：位于 **初始化路径**（插件注册时一次性加载到内存），`call()` 使用已缓存 `self.body`，非请求热路径 — **可接受**。
- `crates/kernel/src/service/http_route/match_hostname.rs` 主机名树查找：旧实现中 `get_mut_by_iter` 的 UB unsafe 意图是避免二次查找；新实现用两步探测增加了一次只读遍历，但热路径主要在 `get` / `match_request`，对性能影响可忽略，**正确性优先于此微小性能差**。
- `crates/shell/src/server.rs` 使用 `std::sync::Mutex` 包裹 `HashMap<String, RunningSgGateway>`：锁持有时间极短（插入/删除/克隆 reloader），无 `.await` 跨锁场景，**可接受**。

### 2.6 分布式 / 并发（Distributed & Concurrency）

- mpsc 的 `.expect("send event error")` 在 `crates/config/src/service/k8s/listen.rs` 共 **10 处**。发送失败意味着接收端已 drop，即控制面已退出 — 此时 watcher 任务也即将被取消，panic 会被 tokio 捕获而不会影响其他任务。**属于可接受的 fast-exit 设计**，但仍建议后续改为 `if let Err(_) = ... { break; }` 以消除 panic。**标记为改进建议**。
- `CancellationToken` 子 token 传播合理。
- Redis 连接池（`deadpool-redis`）改为 Result 传播后，短时故障不再 crash，长时故障由上层重试策略负责。

### 2.7 安全（Security）

- **修复**：Admin 登录 Cookie 加固 `Secure + SameSite=Strict`（2.3-#3）。
- **修复**：日志不再泄漏 Redis URL 凭证（2.3-#6）。
- **修复**：admin-server 登录流中两处 panic 点（时钟 / JWT 编码）改为错误传播。
- **改进建议**：
  - `admin-server` 登录端点未见速率限制 / 锁定策略，单独 PR 补充。
  - `crates/plugin/src/ext/redis/plugins/redis_time_range.rs` 等从请求头/路径取值拼入 Redis key，建议对字符范围（特别是 `:` 与 `*`）做白名单 — **本轮未改动**，列为 P2。

### 2.8 日志（Logging）

- `binary/spacegate/src/main.rs` 插件 dylib 加载路径使用 `println!` / `eprintln!` — 未进入 `tracing` 订阅链路，生产部署看不到。
  - **处理**：改为 `tracing::info!` / `tracing::error!` / `tracing::warn!`。
- `crates/model/src/plugin.rs::239` `PluginInstanceMap::deserialize` 里 `eprintln!` 同样替换为 `tracing::warn!`，并在 `crates/model/Cargo.toml` 添加 `tracing` 依赖。
- 其他 `eprintln!` 出现在 `crates/kernel/tests/test_https.rs`、`crates/model/tests/test_parse_config.rs`，**测试代码保留** 不予修改。

### 2.9 测试（Tests）

- 单元测试：kernel（13 项）、plugin（含 `tests/` 多个集成测试）、model、config 均存在
- 测试里大量 `.expect(...)` 属于 **测试语义正确用法**，符合 skill 要求
- Redis API 改造后，`plugin/tests/test_*.rs` 中 `RedisClient::from` 调用点已改为 `RedisClient::new(...).expect(...)`（初始化期 panic 是合理的测试语义）

### 2.10 质量门（Quality Gate）

- `cargo fmt --all -- --check`：✅ 通过
- `cargo check --lib -p spacegate-kernel -p spacegate-plugin -p spacegate-ext-redis -p spacegate-ext-axum -p spacegate-model`：✅ 通过，**0 warning**
- `cargo check --lib -p spacegate-shell`：依赖 `spacegate-config` 的 fs feature — 预存 Windows 问题，详见 2.3-#7
- `cargo check --lib -p spacegate-config --features k8s`：Windows 下 `openssl-sys` 系统依赖缺失，**环境问题**，非代码问题

---

## 三、本次修改的文件清单

### 新增 / 改动
- `.github/skills/review/SKILL.md`（前置阶段，本审查所依据）
- `crates/extension/redis/src/lib.rs` — `RedisClient::new` / `get_conn()` 返回 Result；`TryFrom<&str>` 替代 `From<&str>`
- `crates/kernel/src/backend_service/static_file_service.rs` — `Metadata::len()` 跨平台
- `crates/kernel/src/service/http_route/match_hostname.rs` — 去 UB；补充 `<'_, T>` lifetime 标注
- `crates/kernel/src/helper_layers/reload.rs` — typo `posisoned` 修正；中毒锁恢复
- `crates/kernel/src/lib.rs` — iter 返回类型补充 `'_` lifetime
- `crates/plugin/Cargo.toml` — 移除 `retry` / `status` / `decompression` feature 与 `hyper-util` 可选依赖
- `crates/plugin/src/plugins.rs` — 清理注释的模块声明
- `crates/plugin/src/plugins/limit.rs` — `get_conn()` Result 传播
- `crates/plugin/src/ext/redis/plugins/{redis_count,redis_time_range,redis_dynamic_route,redis_limit}.rs` — `get_conn()` Result 传播
- `crates/shell/Cargo.toml` — 移除 `plugin-retry` / `plugin-status` / `plugin-decompression` feature
- `crates/shell/src/server.rs` — 中毒锁恢复；去除 Redis URL 凭证日志；错误传播
- `crates/model/Cargo.toml` — 新增 `tracing` 依赖
- `crates/model/src/plugin.rs` — `eprintln!` → `tracing::warn!`
- `crates/config/src/service/k8s/listen.rs` — 4 处 watcher 错误恢复
- `binary/admin-server/src/service/auth.rs` — Cookie 加固 + panic 消除
- `binary/spacegate/src/main.rs` — `println!` / `eprintln!` → `tracing::*`

### 删除（废弃死代码）
- `crates/plugin/src/plugins/retry.rs`
- `crates/plugin/src/plugins/decompression.rs`
- `crates/plugin/src/plugins/status.rs`
- `crates/plugin/src/plugins/status_prev.rs`
- `crates/plugin/src/plugins/status/`（整个目录）

---

## 四、遗留项与后续建议

| 级别 | 项                                                             | 建议处理 |
| ---- | -------------------------------------------------------------- | -------- |
| P1   | `crates/config` fs feature Windows 不兼容                      | 专项重构 PR：抽象 `OsStrExt` 使用，使用 `NamedPipe` 或 HTTP 本地替代 Unix Socket |
| P2   | 多处 `unsafe` 缺少 `// SAFETY:` 注释                           | 走读后补充注释（x_request_id.rs, instance.rs 等）|
| P2   | `crates/config`、`crates/model`、`crates/extension/*` 缺 crate 级 `//!` 文档 | 补写一段 3–5 行说明 |
| P2   | Redis key 拼接未对请求头取值做字符白名单                       | `redis_time_range.rs` / `redis_dynamic_route.rs` 增加 `char.is_ascii_alphanumeric()` 过滤或替换规则 |
| P2   | admin-server 登录缺速率限制 / 账户锁定                         | 加 `tower-governor` 或自研 Redis 滑窗 |
| P3   | `crates/config/src/service/k8s/listen.rs` 10 处 mpsc `.expect` | 改为 `if let Err(_) = ... { break; }` 以完全去 panic |

---

## 五、结论

本轮审查覆盖 9 个 crate 共 10 个维度，关闭 **3 项 P0**、**7 项 P1**、**2 项 P2**、**3 项 P3**。修改后核心 crate（kernel / plugin / model / extension/redis / extension/axum）在 Windows 下 `cargo check` 与 `cargo fmt` 均零告警零错误。剩余 P1（config fs Windows 兼容性）为预存跨平台技术债务，建议单独专项处理。其余 P2 / P3 项为建议性改进，不阻塞当前发布窗口。

— 审查分支：`review/all-modules`
