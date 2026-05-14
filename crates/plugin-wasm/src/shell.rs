//! `Plugin` 实现：在 `call` 内异步驱动 wasm Vm，按 fail_strategy 处理 Trap。
//!
//! 首版未做 VM 池：每次请求新建 Vm。hai-process-mix 实例化 + 配置一遍约几毫秒；
//! 后续按演进文档 §4.3 接 `VmPool` 即可显著降损。

use std::sync::Arc;

use spacegate_kernel::{SgBody, SgRequest, SgResponse};
use spacegate_plugin::{BoxError, Inner, Plugin, PluginConfig};

use crate::config::{FailStrategy, WasmPluginShellConfig};
use crate::runtime::default_module_cache;
use crate::vm::Vm;

/// Proxy-Wasm 宿主壳插件（`CODE = "wasm"`）。
pub struct WasmPluginShell {
    cfg: Arc<WasmPluginShellConfig>,
    module: Arc<wasmtime::Module>,
}

impl Plugin for WasmPluginShell {
    const CODE: &'static str = "wasm";

    fn call(&self, req: SgRequest, inner: Inner) -> impl std::future::Future<Output = Result<SgResponse, BoxError>> + Send {
        let cfg = self.cfg.clone();
        let module = self.module.clone();
        async move {
            tracing::info!(
                target: "spacegate_plugin_wasm",
                method = %req.method(),
                uri = %req.uri(),
                "wasm plugin shell: request entered plugin layer"
            );
            let vm_res = Vm::new(&module, cfg.clone()).await;
            let mut vm = match vm_res {
                Ok(v) => v,
                Err(e) => {
                    tracing::error!(target: "spacegate_plugin_wasm", error = %e, "Vm::new failed");
                    return Ok(passthrough_on_error(e.to_string(), req, inner, cfg.fail_strategy).await);
                }
            };
            tracing::info!(target: "spacegate_plugin_wasm", "Vm initialized, entering process");
            match vm.process(req, inner).await {
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
        let module = cache.get_or_compile(cfg.url.trim()).map_err(|e| -> BoxError { format!("compile wasm: {e}").into() })?;
        Ok(Self {
            cfg: Arc::new(cfg),
            module,
        })
    }
}

/// 当 Vm::new 失败时，原 `req` 已经被消费；按 fail_strategy 合成一个最简单的兜底响应。
async fn passthrough_on_error(err: String, _req: SgRequest, _inner: Inner, fs: FailStrategy) -> SgResponse {
    let status = match fs {
        FailStrategy::FailOpen => http::StatusCode::BAD_GATEWAY,
        FailStrategy::FailClose => http::StatusCode::INTERNAL_SERVER_ERROR,
    };
    let mut resp = SgResponse::new(SgBody::full(format!("wasm plugin init failed: {err}")));
    *resp.status_mut() = status;
    resp
}
