//! SpaceGate **proxy-wasm（wasmtime）** 宿主 crate。
//!
//! **集成方式**：不要从 `spacegate-plugin` 依赖本 crate（会形成循环依赖），应启用 `spacegate-shell` 的 `plugin-wasm`
//! feature；`spacegate_shell::startup` 会在网关启动时调用 [`register`]。
//!
//! 当前实现度（对照 `spacegate演进方案-引入proxy-wasm.md` §4）：
//!
//! - ✅ [`WasmPluginShell`]：`Plugin::CODE = "wasm"`，`call` 真正驱动 wasm VM
//! - ✅ [`engine`] / [`runtime`]：进程级 wasmtime Engine + Module 缓存（按 url）
//! - ✅ [`vm::Vm`]：单 VM 异步状态机；驱动 `proxy_on_request_headers` → `proxy_on_http_call_response` → inner.call → `proxy_on_response_headers/body/log/done/delete`
//! - ✅ [`host_fn`]：proxy-wasm ABI 0.2.1 必要子集（log/time/header/buffer/property/local_response/dispatch_http_call/tick/continue/done）
//! - ⏳ 未做：VmPool（每请求新建 Vm）、ScanningBody（流式 SSE 截断）、fuel/epoch 资源隔离

#![deny(clippy::unwrap_used, clippy::dbg_macro)]

pub mod abi;
pub mod config;
pub mod engine;
pub mod error;
pub mod fetch;
pub mod host_fn;
pub mod host_state;
pub mod runtime;
pub mod shell;
pub mod vm;

pub use config::WasmPluginShellConfig;
pub use shell::WasmPluginShell;

use spacegate_plugin::PluginRepository;

/// 向仓库注册 `wasm` 插件类型（需在 `register_prelude` 或启动逻辑中调用一次）。
pub fn register(repo: &PluginRepository) {
    repo.register::<WasmPluginShell>();
}
