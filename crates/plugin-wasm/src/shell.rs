//! `Plugin` 实现：实例化一个或多个长生命 Vm，后续请求复用，并为每个 Vm 起一条后台 tick 任务驱动 `proxy_on_tick`。
//!
//! 与「每请求新建 Vm」相比的取舍：
//!
//! - 优点：guest 的 root context 可保留状态；`proxy_on_tick` 可真正按 `proxy_set_tick_period_milliseconds` 周期触发；
//!   `on_vm_start` / `on_configure` 仅跑一次，热路径少几毫秒。
//! - 单个 `Vm` 内仍通过 `tokio::sync::Mutex` 串行化处理（wasmtime `Store` 是 !Sync）；
//!   配置 `vm_pool_size > 1` 时，通过多个独立 `Store + Instance` 提供插件实例内并发。
//! - 配置 `wait_vm_pool_size > 0` 时，`X-RateLimit-Policy: wait` 请求会进入独立 wait 池；
//!   其他请求继续走普通池。

use std::sync::{
    atomic::{AtomicUsize, Ordering},
    Arc,
};
use std::time::{Duration, Instant};

use spacegate_kernel::{SgBody, SgRequest, SgResponse};
use spacegate_plugin::{BoxError, Inner, Plugin, PluginConfig};
use tokio::sync::{Mutex as AsyncMutex, MutexGuard};

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

#[derive(Clone)]
struct VmSlot {
    vm: Arc<AsyncMutex<Vm>>,
    inflight: Arc<AtomicUsize>,
}

impl VmSlot {
    fn new(vm: Vm) -> Self {
        Self {
            vm: Arc::new(AsyncMutex::new(vm)),
            inflight: Arc::new(AtomicUsize::new(0)),
        }
    }
}

struct InflightGuard {
    inflight: Arc<AtomicUsize>,
    pool_name: &'static str,
    vm_index: usize,
}

impl InflightGuard {
    fn new(slot: &VmSlot, pool_name: &'static str, vm_index: usize) -> Self {
        let current = slot.inflight.fetch_add(1, Ordering::AcqRel) + 1;
        tracing::debug!(target: "spacegate_plugin_wasm", vm_pool = pool_name, vm_index, inflight = current, "VM inflight incremented");
        Self {
            inflight: slot.inflight.clone(),
            pool_name,
            vm_index,
        }
    }
}

impl Drop for InflightGuard {
    fn drop(&mut self) {
        let current = self.inflight.fetch_sub(1, Ordering::AcqRel).saturating_sub(1);
        tracing::debug!(target: "spacegate_plugin_wasm", vm_pool = self.pool_name, vm_index = self.vm_index, inflight = current, "VM inflight decremented");
    }
}

/// Proxy-Wasm 宿主壳插件（`CODE = "wasm"`）。
pub struct WasmPluginShell {
    cfg: Arc<WasmPluginShellConfig>,
    #[allow(dead_code)]
    module: Arc<wasmtime::Module>,
    vms: Vec<VmSlot>,
    wait_vms: Vec<VmSlot>,
    next_vm: AtomicUsize,
    next_wait_vm: AtomicUsize,
    /// 后台 tick 任务句柄；shell drop 时自动 abort。
    /// `None` 表示创建时没有 tokio runtime 上下文（非测试常见路径），tick 退化为不驱动。
    _tick_tasks: Vec<AbortOnDrop>,
}

impl Plugin for WasmPluginShell {
    const CODE: &'static str = "wasm";

    fn call(&self, req: SgRequest, inner: Inner) -> impl std::future::Future<Output = Result<SgResponse, BoxError>> + Send {
        let cfg = self.cfg.clone();
        let use_wait_pool = is_wait_policy(&req) && !self.wait_vms.is_empty();
        let pool_name = if use_wait_pool { "wait" } else { "normal" };
        let slots = if use_wait_pool { self.wait_vms.clone() } else { self.vms.clone() };
        let module = self.module.clone();
        let start_index = if use_wait_pool {
            self.next_wait_vm.fetch_add(1, Ordering::Relaxed)
        } else {
            self.next_vm.fetch_add(1, Ordering::Relaxed)
        };
        async move {
            tracing::info!(
                target: "spacegate_plugin_wasm",
                method = %req.method(),
                uri = %req.uri(),
                vm_pool = pool_name,
                "wasm plugin shell: request entered plugin layer"
            );

            if slots.is_empty() {
                let mut resp = SgResponse::new(SgBody::full(format!("wasm plugin error: empty {pool_name} VM pool")));
                *resp.status_mut() = http::StatusCode::INTERNAL_SERVER_ERROR;
                return Ok(resp);
            }

            for offset in 0..slots.len() {
                let index = start_index.wrapping_add(offset) % slots.len();
                if let Ok(guard) = slots[index].vm.try_lock() {
                    let _inflight = InflightGuard::new(&slots[index], pool_name, index);
                    return process_with_vm(module, cfg, req, inner, guard, pool_name, index).await;
                }
            }

            let index = start_index % slots.len();
            let _inflight = InflightGuard::new(&slots[index], pool_name, index);
            let guard = slots[index].vm.lock().await;
            process_with_vm(module, cfg, req, inner, guard, pool_name, index).await
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
        let pool_size = cfg.normalized_vm_pool_size();
        let wait_pool_size = cfg.normalized_wait_vm_pool_size();
        let mut vms = Vec::with_capacity(pool_size);
        let mut wait_vms = Vec::with_capacity(wait_pool_size);
        let mut tick_tasks = Vec::with_capacity(pool_size + wait_pool_size);
        for index in 0..pool_size {
            let vm = Vm::new(&module, cfg.clone()).map_err(|e| -> BoxError { format!("Vm::new[{index}]: {e}").into() })?;
            let slot = VmSlot::new(vm);
            if let Some(task) = spawn_tick_loop("normal", index, &slot.vm) {
                tick_tasks.push(task);
            }
            vms.push(slot);
        }
        for index in 0..wait_pool_size {
            let vm = Vm::new(&module, cfg.clone()).map_err(|e| -> BoxError { format!("Vm::new[wait:{index}]: {e}").into() })?;
            let slot = VmSlot::new(vm);
            if let Some(task) = spawn_tick_loop("wait", index, &slot.vm) {
                tick_tasks.push(task);
            }
            wait_vms.push(slot);
        }
        tracing::info!(
            target: "spacegate_plugin_wasm",
            pool_size,
            wait_pool_size,
            "wasm plugin: VM pools created"
        );
        Ok(Self {
            cfg,
            module,
            vms,
            wait_vms,
            next_vm: AtomicUsize::new(0),
            next_wait_vm: AtomicUsize::new(0),
            _tick_tasks: tick_tasks,
        })
    }
}

fn is_wait_policy(req: &SgRequest) -> bool {
    req.headers().get("x-ratelimit-policy").and_then(|value| value.to_str().ok()).map(|value| value.trim().eq_ignore_ascii_case("wait")).unwrap_or(false)
}

async fn process_with_vm(
    module: Arc<wasmtime::Module>,
    cfg: Arc<WasmPluginShellConfig>,
    req: SgRequest,
    inner: Inner,
    mut guard: MutexGuard<'_, Vm>,
    pool_name: &'static str,
    vm_index: usize,
) -> Result<SgResponse, BoxError> {
    match guard.process(req, inner).await {
        Ok(resp) => {
            tracing::info!(target: "spacegate_plugin_wasm", vm_pool = pool_name, vm_index, status = %resp.status(), "Vm::process ok");
            Ok(resp)
        }
        Err(e) => {
            tracing::error!(target: "spacegate_plugin_wasm", vm_pool = pool_name, vm_index, error = %e, "wasm plugin failed");
            if e.requires_vm_rebuild() {
                match Vm::new(&module, cfg.clone()) {
                    Ok(new_vm) => {
                        *guard = new_vm;
                        tracing::warn!(target: "spacegate_plugin_wasm", vm_pool = pool_name, vm_index, "VM rebuilt after abnormal failure");
                    }
                    Err(rebuild_err) => {
                        tracing::error!(
                            target: "spacegate_plugin_wasm",
                            vm_pool = pool_name,
                            vm_index,
                            error = %rebuild_err,
                            "VM rebuild failed after abnormal failure"
                        );
                    }
                }
            }
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

/// 起一条 50ms 粒度的轮询任务：每个 tick 看一眼 `Vm::tick_period_ms()`，到点了就 `Vm::tick()`。
///
/// - 粒度 50ms 是工程取舍：spec 没有规定 tick 必须精确，envoy 也是大颗粒度；如果 guest 设置 < 50ms 的周期，
///   实际触发率会被压到 50ms 一次——记入 `lib.rs` 顶部已知限制。
/// - 任务持有 `Arc<Mutex<Vm>>`，shell drop 时 `AbortOnDrop` 立刻 abort，不存在悬挂任务。
/// - 若 `proxy_on_tick` trap，记 error 后退出循环（防止热循环 panic）。
fn spawn_tick_loop(pool_name: &'static str, vm_index: usize, vm: &Arc<AsyncMutex<Vm>>) -> Option<AbortOnDrop> {
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
                    vm_pool = pool_name,
                    vm_index,
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
