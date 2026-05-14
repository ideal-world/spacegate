//! WASM 模块编译与按 URL 缓存（减少同一 `url` 重复编译）。

use std::sync::Arc;

use moka::sync::Cache;
use once_cell::sync::OnceCell;
use wasmtime::Module;

use crate::engine::shared_engine;
use crate::error::WasmHostError;
use crate::fetch::fetch_wasm_bytes_sync;

/// 进程内模块缓存（键：wasm `url` 字符串）。
pub struct WasmModuleCache {
    engine: &'static wasmtime::Engine,
    inner: Cache<String, Arc<Module>>,
}

impl WasmModuleCache {
    pub fn new(max_entries: u64) -> Self {
        Self {
            engine: shared_engine(),
            inner: Cache::new(max_entries),
        }
    }

    /// 拉取字节并编译；命中缓存则直接返回 `Arc<Module>`。
    pub fn get_or_compile(&self, url: &str) -> Result<Arc<Module>, WasmHostError> {
        let key = url.to_string();
        if let Some(m) = self.inner.get(&key) {
            return Ok(m);
        }
        let bytes = fetch_wasm_bytes_sync(url)?;
        let m = Arc::new(Module::new(self.engine, &bytes)?);
        self.inner.insert(key, m.clone());
        Ok(m)
    }
}

static CACHE: OnceCell<WasmModuleCache> = OnceCell::new();

/// 默认缓存（容量 64）；多实例同 URL 共享编译结果。
pub fn default_module_cache() -> &'static WasmModuleCache {
    CACHE.get_or_init(|| WasmModuleCache::new(64))
}
