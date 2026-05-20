//! 共享 `wasmtime::Engine`：同进程内所有 wasm 插件实例共用。
//!
//! **同步模式**：host fn 是 sync，故不能开 `async_support`——否则 host fn 内
//! 调 guest 的 `proxy_on_memory_allocate` 会 panic「must use `call_async` with async stores」。
//! `proxy_http_call` 的异步语义通过 `tokio::spawn` + mpsc channel 实现，
//! 不需要把整个 store 切到 async。
//!
use once_cell::sync::OnceCell;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;
use wasmtime::{Config, Engine};

static ENGINE: OnceCell<Engine> = OnceCell::new();
static EPOCH_TICKER_STARTED: AtomicBool = AtomicBool::new(false);

/// 进程级单例 Engine（multi-memory 开，async 关）。
pub fn shared_engine() -> &'static Engine {
    ENGINE.get_or_init(|| {
        let mut cfg = Config::new();
        cfg.wasm_multi_memory(true);
        cfg.consume_fuel(true);
        cfg.epoch_interruption(true);
        cfg.async_support(false);
        Engine::new(&cfg).expect("wasmtime Engine::new")
    })
}

/// 启动一个进程级 epoch ticker。每 1ms 递增一次 Engine epoch，配合 Store epoch deadline
/// 给同步 guest hook 提供粗粒度墙钟超时保护。
pub fn ensure_epoch_ticker_started() {
    if EPOCH_TICKER_STARTED.load(Ordering::Acquire) {
        return;
    }
    let Ok(handle) = tokio::runtime::Handle::try_current() else {
        return;
    };
    if EPOCH_TICKER_STARTED.compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire).is_err() {
        return;
    }
    let engine = shared_engine().clone();
    handle.spawn(async move {
        let mut interval = tokio::time::interval(Duration::from_millis(1));
        loop {
            interval.tick().await;
            engine.increment_epoch();
        }
    });
}
