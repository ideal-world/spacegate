# Spacegate 性能与安全专项审查报告

- 分支：`review/all-modules`
- 审查范围：`crates/kernel`、`crates/plugin`、`crates/config`、`crates/shell`、`crates/extension/redis`、`binary/admin-server`
- 依据：`.github/skills/review/SKILL.md` §6（性能）与 §8（安全）
- 范围说明：本报告为在 `REVIEW_REPORT.md` 基础上 **针对性能 / 安全的深挖**，聚焦在「会在请求热路径或生产暴露面上真正出问题」的点，不重复已在前一轮修复的条目。

---

## 一、总体结论

| 等级 | 性能 | 安全                                                                                     |
| ---- | ---- | ---------------------------------------------------------------------------------------- |
| P0   | 0    | **1** — 默认后端 HTTP 客户端关闭 TLS 证书校验                                            |
| P1   | 2    | **4** — admin-server 明文 HTTP / 无请求体大小限制 / 无登录限速 / kernel 无全局 body 上限 |
| P2   | 3    | 3                                                                                        |
| P3   | 2    | 2                                                                                        |

最严重的一条是 **P0-SEC-1**：在任何未显式配置自定义 `rustls::ClientConfig` 的部署中，spacegate 作为反向代理向后端建立 HTTPS 连接时会 **跳过所有证书校验**，完全可被中间人。

---

## 二、安全（Security）

### P0-SEC-1 — 默认后端 HTTPS 客户端关闭证书校验（MITM 风险）

**位置**：[crates/kernel/src/backend_service/http_client_service.rs](crates/kernel/src/backend_service/http_client_service.rs#L67-L76)

```rust
fn get_rustls_config_dangerous() -> rustls::ClientConfig {
    let store = rustls::RootCertStore::empty();
    ...
    let mut dangerous_config = rustls::ClientConfig::dangerous(&mut config);
    dangerous_config.set_certificate_verifier(Arc::new(NoCertificateVerification {}));
    config
}
...
impl Default for ClientRepo {
    fn default() -> Self {
        let config = get_rustls_config_dangerous();   // ← default 调用 dangerous
        let default = HttpClient::new(config);
        ...
    }
}
```

- `ClientRepo::default()` 是 `ClientRepo::global()` 的初始化值。只要未手动调用 `set_global_default` 注入一个安全的 `HttpClient`，所有走 `get_client()`、`get_or_default` 的后端请求 **都会接受任意伪造证书**。
- `NoCertificateVerification` 的 `verify_server_cert` 直接 `Ok(ServerCertVerified::assertion())`，对主机名、CA、签名、到期一律放行。
- 该 ClientRepo 被 `http_client_service` 作为所有 HTTPS backend 请求的默认出口使用。
- **攻击面**：对手只要能在网关与后端之间插入流量（同机房二层劫持、BGP 劫持、DNS 劫持、服务网格 sidecar 被攻破），就能冒充任意后端接收请求、改造响应、窃取 token。
- **建议**：
  1. `ClientRepo::default()` 改为使用 `rustls::ClientConfig::builder().with_native_roots().with_no_client_auth()`（fallback 到 `webpki-roots` 也可）。
  2. 保留 `HttpClient::new_dangerous()` 作为显式 opt-in，并在注释中警告「仅用于受控测试或内网全白名单」。
  3. 在 `ClientRepo` 文档中强调 `set_global_default` 的必要性。
  4. 建议增加 `SGE_ALLOW_INSECURE_BACKEND` 或类似环境变量做最后一道闸门，默认关闭。

---

### P1-SEC-2 — admin-server 监听明文 HTTP

**位置**：[binary/admin-server/src/main.rs](binary/admin-server/src/main.rs#L40-L48)

```rust
let listener = tokio::net::TcpListener::bind(addr).await?;
axum::serve(listener, ...).await?;
```

- admin-server 承载登录（JWT 签发）、配置写入（gateway / route / plugin CRUD）、密钥摘要比对，却完全走明文 HTTP。
- 上一轮我们已给登录 Cookie 加上 `Secure; SameSite=Strict`，但 **当连接是 HTTP 时浏览器不会发送 `Secure` Cookie**，整个认证与授权链路在明文网络里暴露 `Authorization: Bearer ...` 和登录请求体 `{ak, sk}`。
- **建议**：
  1. 通过 clap 参数或配置文件增加 `--tls-cert`、`--tls-key`，默认要求启用 rustls 监听（`axum-server` 或 `hyper-util` + `tokio-rustls`）。
  2. 如需保持 plain HTTP（比如部署在 K8s mTLS 服务网格内），要求显式 `--allow-insecure`，并在启动日志中 `tracing::warn!` 提示。

---

### P1-SEC-3 — admin-server 无请求体大小 / 连接超时限制

**位置**：[binary/admin-server/src/main.rs](binary/admin-server/src/main.rs#L40-L48)、登录端点 [binary/admin-server/src/service/auth.rs](binary/admin-server/src/service/auth.rs)

- axum 默认 `Json<T>` extractor 读取整个请求体到内存；未加 `tower-http::limit::RequestBodyLimitLayer`、未加 `tower::timeout::TimeoutLayer`、未加 `axum::extract::DefaultBodyLimit`（axum 0.6+ 默认 2 MiB，但 admin-server 使用 axum 0.7 需手动确认）。
- 攻击者可发送 1 GB `Content-Length` 的 `POST /login`，结合多并发耗尽进程内存；或发送慢速连接占满 FD。
- **建议**：
  ```rust
  let app = app
      .layer(axum::extract::DefaultBodyLimit::max(1 << 20))      // 1 MiB
      .layer(tower::timeout::TimeoutLayer::new(Duration::from_secs(30)));
  ```
  并在启动时通过 `tower-http::limit::RequestBodyLimitLayer` 强制每个路由上限。

---

### P1-SEC-4 — admin-server 登录无速率限制 / 锁定

**位置**：[binary/admin-server/src/service/auth.rs](binary/admin-server/src/service/auth.rs)

- `login` 端点直接比较 `sk` 的 SHA-256 摘要，无失败计数、无 IP 限速、无账户锁定。
- 虽然 SHA-256(sk) 抗撞攻击成本高，但若 `sk` 是人类可读口令，离线字典攻击对 SHA-256（无盐）成本极低，线上只要能持续试就可能命中。
- **建议**：
  1. 将 `sk_digest` 的对比改为 **constant-time**（`subtle::ConstantTimeEq`），消除时序侧信道。
  2. 增加 tower middleware 做 IP/账户级滑窗限速（`tower-governor` 或基于 Redis 的自写中间件）。
  3. 更长期：存储改用 `argon2`/`bcrypt` 加盐哈希替代无盐 SHA-256。

---

### P1-SEC-5 — kernel 无全局请求 body 上限

**位置**：`crates/kernel/src/service/http_gateway.rs` 及其 `listen.rs`、`SgBody`

- 在 kernel 中搜 `RequestBodyLimit|BodyLimit|max_body|max_frame` 无任何匹配。hyper v1 的 `http1::Builder` / `http2::Builder` 默认不对请求 body 大小做限制。
- 对于 gateway：客户端可发起无限大小 `Transfer-Encoding: chunked` 请求；任何后续插件或插件里 `body.collect().await` 会把整条流拉入内存，耗尽堆。
- **建议**：在 `SgListen` / `Gateway::builder` 提供 `.max_request_body_size(Option<usize>)`，通过 `hyper_util::server::conn::auto::Builder::max_body_size` 或在首个 tower Layer 中用 `http_body_util::Limited<B>` 包一层。对 `SgBody` 默认值建议 8–16 MiB，可由 gateway 配置覆盖。

---

### P2-SEC-6 — static_file_service 的 canonicalize 失败回退不够稳健

**位置**：[crates/kernel/src/backend_service/static_file_service.rs](crates/kernel/src/backend_service/static_file_service.rs#L51-L57)

```rust
let path = dir.join(request.uri().path().trim_start_matches('/'))
              .canonicalize()
              .unwrap_or(dir.to_owned());
if !path.starts_with(dir) { return 404; }
```

- 当 `canonicalize()` 失败（文件不存在、权限不足、Windows 上带 `\\?\` 前缀问题等）时 fallback 到 `dir` 本身；后续 `File::open(dir)` 会得到目录，并进入分支返回 303 → `/index.html`。对于不存在的文件应当返回 404 而不是重定向，当前行为会把 **任何不存在路径的请求都重定向到 /index.html**，这对 SPA 是功能特性但对纯静态资源目录是信息泄漏（告诉外部 `/index.html` 存在）。
- 更重要的是，`canonicalize` 在某些 Windows / 符号链接 / 容器挂载场景会返回 UNC 路径（`\\?\C:\...`），此时 `starts_with(dir)` 可能因 `dir` 为 `C:\...` 而 **判空失败**，把合法文件当成越权过滤掉；反之亦可能因前缀匹配误判通过。
- **建议**：
  1. canonicalize 失败时直接 `404`，不要回退到 `dir`。
  2. 对 `dir` 本身预先 `canonicalize()` 一次缓存，比较时用 canonical vs canonical。
  3. 在 Windows 上显式 strip `\\?\` 前缀后再比较，或使用 `same_file::is_same_file`。

---

### P2-SEC-7 — redis 插件的 key 拼接未限制 header 值字符

**位置**：[crates/plugin/src/ext/redis/plugins.rs](crates/plugin/src/ext/redis/plugins.rs#L9-L23)

```rust
let header = req.headers().get(header).and_then(|v| v.to_str().ok())?;
Some(format!("{}:{}:{}", method, path, header))
```

- 被 `redis_count`、`redis_time_range`、`redis_limit`、`redis_dynamic_route` 共用。
- RESP 协议本身通过长度前缀传递，不存在「Redis 命令注入」，但 **键名逻辑层的冒号 `:` 用作分隔符**，而 `HeaderValue::to_str()` 放行任何 ASCII 可见字符（含 `:`）。
- 攻击面（仅 `redis_dynamic_route` 危险）：插件用 `format!("{}:{}", prefix, key)` 再读取后端 `domain`。攻击者若能同时控制路由/方法匹配与 header 值，可构造 header 值为 `*:/other-prefix:victim-key`，引导插件读到原本属于另一个 `{prefix'}:{method}:{path}:{header}` 条目的 domain，等同把流量路由到 **任意由管理员在 Redis 中登记过的后端域名**（管理员本意绑定给另一 header 值的域名）。
- **建议**：
  1. 对 header 值做白名单过滤（`A-Za-z0-9_-.` 等），遇到其他字符返回 401。
  2. 对 domain 解出后额外做白名单校验（参见 P2-SEC-8）。

---

### P2-SEC-8 — redis_dynamic_route 的 domain 未限制为白名单

**位置**：[crates/plugin/src/ext/redis/plugins/redis_dynamic_route.rs](crates/plugin/src/ext/redis/plugins/redis_dynamic_route.rs#L50-L73)

- 插件从 Redis 读 `domain`，直接拆 `scheme://host`，拼入出站 URI；结合 P0-SEC-1 默认 HTTPS 不验证证书，相当于把 gateway 变成 **由 Redis 控制的开放代理**。
- 若 Redis 被未授权写入（管理面权限、Redis 同实例跨业务共用、或 key 冲突如 P2-SEC-7），攻击者可把某个路由后端改成内网 `http://169.254.169.254/`（云元数据）、内部管理网段、`file://` 类 scheme-confusion（尽管本插件过滤了后者）等。
- **建议**：
  1. `RedisDynamicRouteConfig` 增加 `allowed_hosts: Vec<String>` 或 `allowed_host_regex: String`，只接受白名单命中的 domain。
  2. 明确只接受 `http` / `https` scheme（当前其实没校验 scheme_str，攻击者可写入 `gopher://...` 之类让 hyper 报错而触发不可控异常）。

---

### P1-SEC-9 — kernel x_request_id.rs `unwrap_unchecked` 引入未定义行为

**位置**：[crates/kernel/src/utils/x_request_id.rs](crates/kernel/src/utils/x_request_id.rs#L44) 以及第 48 行

```rust
let ts_id = unsafe { SystemTime::now().duration_since(UNIX_EPOCH).unwrap_unchecked().as_millis() as u64 } << 22;
...
unsafe { HeaderValue::from_str(&format!("{:016x}", id)).unwrap_unchecked() }
```

- `duration_since(UNIX_EPOCH)` 在系统时钟被人为调到 1970 之前时返回 `Err`，`unwrap_unchecked` 上 **直接 UB**（不是 panic 而是未定义行为）。
- `HeaderValue::from_str(format!("{:016x}"))` 虽然输入永远是十六进制，`unwrap_unchecked` 在代码层确实安全，但缺 SAFETY 注释，且一旦有人重构 format 串（例如加空格或非 ASCII 分隔）就会悄悄变成 UB。
- **建议**：
  ```rust
  let now_ms = SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_millis() as u64).unwrap_or(0);
  let ts_id = now_ms << 22;
  ...
  HeaderValue::from_str(&format!("{:016x}", id)).unwrap_or_else(|_| HeaderValue::from_static("0"))
  ```
  `unsafe` 块要么去掉，要么补 `// SAFETY:` 注释解释为什么 `unwrap_unchecked` 成立。

---

### P2-SEC-10 — helper_layers/function.rs 与 http_route.rs 的 `unwrap_unchecked`

- [crates/kernel/src/helper_layers/function.rs](crates/kernel/src/helper_layers/function.rs#L231) `Inner::call` 对 `ArcHyperService::call` 的 `Error` 做 `unwrap_unchecked`，依赖 `Error = Infallible`。
- [crates/kernel/src/service/http_route.rs](crates/kernel/src/service/http_route.rs#L229-L233) 对 `http_backend_service` 返回做 `unwrap_unchecked`。
- 如果将来有人把 backend service 的 `Error` 从 `Infallible` 改为别的类型，这里会变成 UB 而且可能编译通过。
- **建议**：改为 `match ... { Ok(r) => r, Err(e) => match e {} }`（对 `Infallible` 的空枚举匹配是零成本等价写法，且编译器会在类型变更时报错），或至少添加 `// SAFETY: inner is ArcHyperService<Response, Infallible>` 注释。

---

## 三、性能（Performance）

### P1-PERF-1 — `http_client_service::ClientRepo` 每次请求取用都要 `Mutex::lock`

**位置**：[crates/kernel/src/backend_service/http_client_service.rs](crates/kernel/src/backend_service/http_client_service.rs#L100-L110)

```rust
pub fn get(&self, code: &str) -> Option<HttpClient> {
    self.repo.lock().expect("failed to lock client repo").get(code).cloned()
}
```

- `ClientRepo.repo` 是 `std::sync::Mutex<HashMap<String, HttpClient>>`。`get_or_default` / `get` 位于每个后端请求路径，所有线程串行抢互斥锁，同时 `ClientRepo::global()` 外层又加了一个 `RwLock`。双层同步且内层 Mutex。
- 客户端对象本身通过 `Arc` 共享，配置变更频率远低于请求频率。
- **建议**：
  1. 把内层 `Mutex<HashMap>` 换成 `ArcSwap<HashMap<String, HttpClient>>` 或 `parking_lot::RwLock`；读路径零竞争克隆 `Arc`，写路径少数几次。
  2. 去掉外层 `RwLock`，让 `ClientRepo` 本身通过 `Arc<ClientRepo>` + 内部 `ArcSwap` 自我可变。

---

### P1-PERF-2 — 静态 mutex 持有期间做 HashMap 查找，可被请求路径放大

**位置**：[crates/kernel/src/extension/defer.rs](crates/kernel/src/extension/defer.rs#L22-L27)

```rust
let mut g = self.mappers.lock().expect("never poisoned");
```

- `Defer::push_back` / `apply` 在每个请求生命周期内都被调用（路由匹配、插件后置动作）。`std::sync::Mutex` 粒度是整个 Defer 容器。
- 虽然临界区短，但在多 vCPU 机器上高并发抢锁会是微延迟瓶颈。
- **建议**：`Defer` 改为 `SmallVec<[BoxedDeferFn; 4]>` 直接挂在 request extension 里，而不是走共享 Mutex；或改 `parking_lot::Mutex`。

---

### P2-PERF-3 — `shell/src/server.rs` 配置重载路径大量 `.clone()`

**位置**：[crates/shell/src/server.rs](crates/shell/src/server.rs#L33-L90)、L249-L350

- 每次 route 重建时对 `gateway_name: Arc<str>`、`route_name: Arc<str>` 多次 `.clone()`（每次 Arc 增减引用计数），对 `HashMap<String, SgHttpRoute>` 做 `.clone()`，对 `config_item.gateway` 整体 clone。
- 不在请求热路径而在 **配置变更** 路径，影响的是重载延迟与短时内存峰值。对于 K8s 场景（watcher 抖动会触发频繁 reload），仍建议优化。
- **建议**：`Arc<str>` 的 clone 本就便宜，可忽略；但 `HashMap` 与 `SgGateway`（含 TLS cert 原始数据）的深克隆建议改为 `Arc<HashMap<...>>` / `Arc<SgGateway>` 共享。

---

### P2-PERF-4 — `crates/plugin/src/plugins/static_resource.rs::create()` 同步 `std::fs::read`

**位置**：[crates/plugin/src/plugins/static_resource.rs](crates/plugin/src/plugins/static_resource.rs#L77)

- 位于插件初始化路径（异步任务 `create()`），当前同步 `fs::read` 不在 `call()` 热路径，属 **可接受**。
- 但 `create()` 在 K8s watcher 路径被异步任务调用；若静态资源文件 >100 MiB 或位于慢盘（NFS），会阻塞 runtime 的一个 worker 数毫秒到数秒。
- **建议**：`create()` 中改用 `tokio::task::spawn_blocking` 包一次 `std::fs::read`，保持初始化语义但让出 runtime。

---

### P2-PERF-5 — redis 插件每次请求 `serde_json::from_value(config.spec.clone())` 被初始化一次

**位置**：[crates/plugin/src/ext/redis/plugins/redis_count.rs](crates/plugin/src/ext/redis/plugins/redis_count.rs#L54)、`redis_dynamic_route.rs:31` 等

- 这些都在 `Plugin::create()` 初始化路径，不在 `call()` 热路径 — **可接受**。
- 顺手提醒：`redis_count::redis_call` 里每个请求打 4 次 Redis round-trip（`EXISTS / SET / INCR / GET`），可用 Lua `EVAL` 合并为 1 次 RTT，降低尾延迟。

---

### P3-PERF-6 — `crates/kernel/src/service/http_route/match_hostname.rs` 安全化后引入额外探测

**位置**：`match_hostname.rs::get_mut_by_iter`（上一轮修复 UB 时改造）

- 新实现通过「先只读探测 `has_child_match`，再下钻」多走一次不可变遍历。对比旧 UB 实现在最坏情况下增加 O(depth) 次只读比较。对一般主机名树（深度 ≤ 10），影响几十纳秒，**可接受**。
- 建议：后续如 profiling 显示热点，可用 `hashbrown::raw_entry_mut` 或切回 `entry().or_insert_with` 的结构性改动。

---

### P3-PERF-7 — `crates/kernel/src/backend_service/http_client_service.rs::HttpClient::request` 在 error 路径把 body 全部读入 `Response::bad_gateway`

- 仅影响错误路径延迟，不影响主流程。低优先级。

---

## 四、不阻塞发布但建议跟进

| 编号  | 类型 | 说明                                                                                                      | 建议                                                                                              |
| ----- | ---- | --------------------------------------------------------------------------------------------------------- | ------------------------------------------------------------------------------------------------- |
| FUP-1 | 观测 | `tracing::instrument` 缺少 `skip_all`、`fields` 少量关键维度（gateway/route/plugin code）                 | 在 kernel 关键 service 的 `call` 上补 `#[instrument(skip_all, fields(gateway=%..., route=%...))]` |
| FUP-2 | 安全 | admin-server 对 Retrieve 接口没有租户边界，任何具备 JWT 的 caller 可读任意 gateway                        | 增加 RBAC/ACL 或至少区分 "read_only" / "admin" role                                               |
| FUP-3 | 安全 | `SgTlsConfig` 中证书/私钥字段应考虑在日志中 `impl Debug` 时脱敏                                           | 自定义 `Debug` 打印 `"<REDACTED>"`                                                                |
| FUP-4 | 性能 | `ArcHyperService` 与 `tower::util::BoxCloneService` 每个 Layer 包裹一次 `Arc<Box<dyn Service>>`，开销累积 | 分析插件 layer 数量，考虑 `tower::ServiceBuilder::into_make_service` 合并                         |

---

## 五、建议的修复批次

优先级排序（按影响面 × 发生概率）：

1. **本轮立即修**（生产安全阻塞）
   - P0-SEC-1 默认 HTTPS 证书校验开关
   - P1-SEC-2 admin-server 默认 TLS
   - P1-SEC-3 admin-server 请求体大小 / 超时
   - P1-SEC-5 kernel 全局 body 上限
   - P1-SEC-9 x_request_id 去 UB

2. **下一迭代**
   - P1-SEC-4 登录限速 / constant-time
   - P2-SEC-6 static_file canonicalize 回退
   - P2-SEC-7 / P2-SEC-8 redis 插件 key 白名单 + domain 白名单
   - P2-SEC-10 `unwrap_unchecked` 替换为 `Infallible` match
   - P1-PERF-1 / P1-PERF-2 Mutex → ArcSwap / parking_lot

3. **长尾**
   - FUP-1..4 与 P3 级性能项

---

*报告写于 `review/all-modules` 分支，基于 commit `d038138` 之上的静态审查。*
