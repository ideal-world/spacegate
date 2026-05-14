//! SpaceGate **proxy-wasm (wasmtime) 宿主** crate。
//!
//! **集成方式**：不要从 `spacegate-plugin` 依赖本 crate（会形成循环依赖），应启用 `spacegate-shell`
//! 的 `plugin-wasm` feature；`spacegate_shell::startup` 会在网关启动时调用 [`register`]。
//!
//! # 与 [proxy-wasm/spec v0.2.1](https://github.com/proxy-wasm/spec) 的覆盖情况
//!
//! ## Host functions (env)
//!
//! - **Integration / Memory management**：guest 导出 `_initialize` 优先，否则回退 `_start`；
//!   allocator 优先 `proxy_on_memory_allocate`，否则回退 `malloc`。
//! - **Logging**：`proxy_log` / `proxy_get_log_level` 完整实现（host tracing 级别映射）。
//! - **Clocks**：`proxy_get_current_time_nanoseconds` + `wasi_snapshot_preview1.clock_time_get`。
//! - **Timers**：`proxy_set_tick_period_milliseconds` 完整生效；`shell.rs` 起一条 50ms 颗粒度的
//!   后台 tokio 任务，到点 → `Vm::tick()` → guest `proxy_on_tick`。这要求 `Plugin::create`
//!   时存在 tokio runtime（spacegate-shell 的标准启动路径）；无 runtime 时降级为不驱动。
//! - **Randomness**：`wasi_snapshot_preview1.random_get` 走 `getrandom`（OS RNG）。
//! - **Environment**：`environ_*` 按 spec 全部返回 0/SUCCESS。
//! - **Buffers**：`proxy_get_buffer_bytes` / `proxy_get_buffer_status` 覆盖
//!   HttpRequestBody / HttpResponseBody / HttpCallResponseBody / Vm/PluginConfiguration；
//!   TCP / gRPC / FFI args 类型按 spec 返回 NotFound。
//!   `proxy_set_buffer_bytes` 实现 prepend / append / inject / replace 语义。
//! - **HTTP fields**：`proxy_get_header_map_size/pairs/value` + add/replace/remove + set_pairs，
//!   覆盖 Request/Response/Trailers + HttpCallResponse Headers/Trailers；GRPC metadata 类型
//!   按 spec 返回 Unimplemented。
//! - **HTTP streams**：`proxy_send_local_response` / `proxy_continue_stream` /
//!   `proxy_close_stream`（TCP downstream/upstream 按 spec 返回 Unimplemented）。
//! - **HTTP calls**：`proxy_http_call`（reqwest 异步、`:method`/`:path`/`:authority` 校验，
//!   按 cluster map 或 `:authority` 兜底解析 URL）。
//! - **Shared K/V**：`proxy_get/set_shared_data` 进程级 RwLock，含 CAS 比对。
//! - **Shared queues**：`proxy_register/resolve/enqueue/dequeue_shared_queue` 进程级 Mutex VecDeque。
//! - **Metrics**：`proxy_define/record/increment/get_metric` 进程级 Counter/Gauge/Histogram。
//! - **Properties**：`proxy_get/set_property` 支持 well-known
//!   (`plugin_name`/`plugin_root_id`/`plugin_vm_id`/`source.address`+`source.port`/
//!   `destination.address`+`destination.port`/`request.protocol`/`request.size`/`request.total_size`/
//!   `response.size`/`response.total_size`) 与用户自定义。
//! - **gRPC**：按 spec 全部 `Unimplemented`。
//! - **Foreign function**：按 spec `NotFound`（无注册表）。
//! - **`proxy_done` / `proxy_set_effective_context`**：完整实现。
//!
//! ## Guest callbacks driven by host
//!
//! - 启动：`_initialize`/`_start` → `proxy_on_context_create(root,0)` →
//!   `proxy_on_vm_start` → `proxy_on_configure`（**仅一次**，由 `WasmPluginShell::create` 执行）。
//! - 每请求：`proxy_on_context_create(http_id, root)` → `proxy_on_request_headers` →
//!   （可选）`proxy_on_request_body` → （可选）`proxy_on_request_trailers` →
//!   `inner.call` → `proxy_on_response_headers` → （可选）`proxy_on_response_body` →
//!   （可选）`proxy_on_response_trailers` → `proxy_on_log` → `proxy_on_done` → `proxy_on_delete`。
//!   `WasmPluginShell` 持有 `Arc<tokio::sync::Mutex<Vm>>`，所有请求串行经过同一 root VM
//!   ——与 envoy/istio 的 per-worker 单线 wasm 模型一致。
//! - 后台 `proxy_on_tick`：`shell.rs` 起 50ms 颗粒度的 tokio 任务驱动；guest 通过
//!   `proxy_set_tick_period_milliseconds` 改周期。
//! - 异步 `proxy_on_http_call_response`：在 Pause 状态机里 await `dispatch_rx` 后回调。
//! - Pause/Continue：`proxy_continue_stream` 同步解除 Pause；多次 dispatch 可串联。
//! - Local response：`proxy_send_local_response` 任意 hook 都能短路。
//!
//! ## 已知尚未驱动的回调（按设计取舍）
//!
//! - TCP 流回调（`proxy_on_new_connection`/`*_downstream_*`/`*_upstream_*`）：
//!   spacegate-kernel 当前是 HTTP-only，TCP 插件层不支持。
//! - `proxy_on_queue_ready` / `proxy_on_grpc_*` / `proxy_on_foreign_function`：
//!   对应 host fn 已为 spec 合规返回值；guest 侧回调不会被触发。

#![deny(clippy::unwrap_used, clippy::dbg_macro)]

pub mod abi;
pub mod config;
pub mod engine;
pub mod error;
pub mod fetch;
pub mod host_fn;
pub mod host_state;
pub mod runtime;
pub mod shared;
pub mod shell;
pub mod vm;

pub use config::WasmPluginShellConfig;
pub use shell::WasmPluginShell;

use spacegate_plugin::PluginRepository;

/// 向仓库注册 `wasm` 插件类型（需在 `register_prelude` 或启动逻辑中调用一次）。
pub fn register(repo: &PluginRepository) {
    repo.register::<WasmPluginShell>();
}
