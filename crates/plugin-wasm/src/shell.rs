//! `Plugin` 实现：实例化一次长生命 Vm，后续请求复用，并起一条后台 tick 任务驱动 `proxy_on_tick`。
//!
//! 与旧版「每请求新建 Vm」相比的取舍：
//!
//! - 优点：guest 的 root context 可保留状态；`proxy_on_tick` 可真正按 `proxy_set_tick_period_milliseconds` 周期触发；
//!   `on_vm_start` / `on_configure` 仅跑一次，热路径少几毫秒。
//! - 代价：所有经过本插件实例的请求会通过同一把 `tokio::sync::Mutex` 串行化处理——
//!   wasmtime `Store` 是 !Sync，无法并发；envoy / istio 的 proxy-wasm 实现也是相同模型。
//!
//! 后续要做更细粒度并发（多 root VM 池）属于演进文档 §4.3 范畴，本版不在范围内。

use std::sync::Arc;
use std::time::{Duration, Instant};

use spacegate_kernel::{SgBody, SgRequest, SgResponse};
use spacegate_plugin::{BoxError, Inner, Plugin, PluginConfig};
use tokio::sync::Mutex as AsyncMutex;

use crate::config::{FailStrategy, WasmPluginShellConfig};
use crate::runtime::default_module_cache;
use crate::vm::Vm;

/// Drop 时 abort 关联的 tokio 任务；保证后台 tick 不会在 shell 析构后继续持有 Vm 引用。
struct AbortOnDrop(tokio::task::JoinHandle<()>);
impl Drop for AbortOnDrop {
    fn drop(&mut self) {
        self.0.abort();
    }
}

/// Proxy-Wasm 宿主壳插件（`CODE = "wasm"`）。
pub struct WasmPluginShell {
    cfg: Arc<WasmPluginShellConfig>,
    #[allow(dead_code)]
    module: Arc<wasmtime::Module>,
    vm: Arc<AsyncMutex<Vm>>,
    /// 后台 tick 任务句柄；shell drop 时自动 abort。
    /// `None` 表示创建时没有 tokio runtime 上下文（非测试常见路径），tick 退化为不驱动。
    _tick_task: Option<AbortOnDrop>,
}

impl Plugin for WasmPluginShell {
    const CODE: &'static str = "wasm";

    fn call(&self, req: SgRequest, inner: Inner) -> impl std::future::Future<Output = Result<SgResponse, BoxError>> + Send {
        let cfg = self.cfg.clone();
        let vm = self.vm.clone();
        async move {
            tracing::info!(
                target: "spacegate_plugin_wasm",
                method = %req.method(),
                uri = %req.uri(),
                "wasm plugin shell: request entered plugin layer"
            );
            let mut guard = vm.lock().await;
            match guard.process(req, inner).await {
                Ok(resp) => {
                    tracing::info!(target: "spacegate_plugin_wasm", status = %resp.status(), "Vm::process ok");
                    Ok(resp)
                }
                Err(e) => {
                    tracing::error!(target: "spacegate_plugin_wasm", error = %e, "wasm plugin failed");
                    let status = if matches!(cfg.fail_strategy, FailStrategy::FailOpen) {
                        http::StatusCode::BAD_GATEWAY
                    } else {
                        http::StatusCode::INTERNAL_SERVER_ERROR
                    };
                    let mut resp = SgResponse::new(SgBody::full(format!("wasm plugin error: {e}")));
                    *resp.status_mut() = status;
                    Ok(resp)
                }
            }
        }
    }

    fn create(plugin_config: PluginConfig) -> Result<Self, BoxError> {
        let raw_spec = plugin_config.spec.clone();
        let cfg: WasmPluginShellConfig = serde_json::from_value(plugin_config.spec).map_err(|e| -> BoxError { format!("wasm spec: {e}").into() })?;
        if cfg.url.trim().is_empty() {
            return Err("wasm plugin: missing or empty `url`".into());
        }
        tracing::info!(
            target: "spacegate_plugin_wasm",
            url = %cfg.url,
            plugin_config_kind = %if cfg.plugin_config.is_null() { "null" } else { "object" },
            plugin_config_keys = ?cfg.plugin_config.as_object().map(|o| o.keys().collect::<Vec<_>>()),
            clusters = ?cfg.clusters.keys().collect::<Vec<_>>(),
            raw_keys = ?raw_spec.as_object().map(|o| o.keys().collect::<Vec<_>>()),
            "wasm plugin: create with config"
        );
        let cache = default_module_cache();
        let module = cache.get_or_compile(&cfg).map_err(|e| -> BoxError { format!("compile wasm: {e}").into() })?;
        let cfg = Arc::new(cfg);
        let vm = Vm::new(&module, cfg.clone()).map_err(|e| -> BoxError { format!("Vm::new: {e}").into() })?;
        let vm = Arc::new(AsyncMutex::new(vm));
        let tick_task = spawn_tick_loop(&vm);
        Ok(Self {
            cfg,
            module,
            vm,
            _tick_task: tick_task,
        })
    }
}

/// 起一条 50ms 粒度的轮询任务：每个 tick 看一眼 `Vm::tick_period_ms()`，到点了就 `Vm::tick()`。
///
/// - 粒度 50ms 是工程取舍：spec 没有规定 tick 必须精确，envoy 也是大颗粒度；如果 guest 设置 < 50ms 的周期，
///   实际触发率会被压到 50ms 一次——记入 `lib.rs` 顶部已知限制。
/// - 任务持有 `Arc<Mutex<Vm>>`，shell drop 时 `AbortOnDrop` 立刻 abort，不存在悬挂任务。
/// - 若 `proxy_on_tick` trap，记 error 后退出循环（防止热循环 panic）。
fn spawn_tick_loop(vm: &Arc<AsyncMutex<Vm>>) -> Option<AbortOnDrop> {
    let handle = tokio::runtime::Handle::try_current().ok()?;
    let vm = vm.clone();
    let task = handle.spawn(async move {
        const POLL_GRANULARITY: Duration = Duration::from_millis(50);
        let mut interval = tokio::time::interval(POLL_GRANULARITY);
        // 首次 tick 立刻就绪——跳过它，避免一启动就触发 on_tick。
        interval.tick().await;
        let mut last_tick: Option<Instant> = None;
        loop {
            interval.tick().await;
            let mut guard = vm.lock().await;
            let period = guard.tick_period_ms();
            if period == 0 {
                last_tick = None;
                continue;
            }
            let due = match last_tick {
                Some(t) => t.elapsed().as_millis() as u64 >= period as u64,
                None => true,
            };
            if !due {
                continue;
            }
            if let Err(e) = guard.tick() {
                tracing::error!(
                    target: "spacegate_plugin_wasm",
                    error = %e,
                    "proxy_on_tick failed; stopping tick task"
                );
                return;
            }
            last_tick = Some(Instant::now());
        }
    });
    Some(AbortOnDrop(task))
}
