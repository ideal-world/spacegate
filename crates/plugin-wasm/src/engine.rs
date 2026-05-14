//! 共享 `wasmtime::Engine`：同进程内所有 wasm 插件实例共用。
//!
//! **同步模式**：host fn 是 sync，故不能开 `async_support`——否则 host fn 内
//! 调 guest 的 `proxy_on_memory_allocate` 会 panic「must use `call_async` with async stores」。
//! `proxy_http_call` 的异步语义通过 `tokio::spawn` + mpsc channel 实现，
//! 不需要把整个 store 切到 async。
//!
//! 资源/超时限制（fuel/epoch）暂未启用：演进文档 §4.7 的"资源/Panic 隔离"
//! 列入后续阶段；本阶段优先保证 hai-process-mix 鉴权流程跑通。

use once_cell::sync::OnceCell;
use wasmtime::{Config, Engine};

static ENGINE: OnceCell<Engine> = OnceCell::new();

/// 进程级单例 Engine（multi-memory 开，async 关）。
pub fn shared_engine() -> &'static Engine {
    ENGINE.get_or_init(|| {
        let mut cfg = Config::new();
        cfg.wasm_multi_memory(true);
        cfg.async_support(false);
        Engine::new(&cfg).expect("wasmtime Engine::new")
    })
}
