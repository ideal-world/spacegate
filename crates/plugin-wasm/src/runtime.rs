//! WASM 模块编译与按 URL 缓存（减少同一 `url` 重复编译）。

use std::sync::Arc;

use moka::sync::Cache;
use once_cell::sync::OnceCell;
use sha2::{Digest, Sha256};
use wasmtime::Module;

use crate::config::WasmPluginShellConfig;
use crate::engine::shared_engine;
use crate::error::WasmHostError;
use crate::fetch::fetch_wasm_bytes_sync_with_auth;

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
    pub fn get_or_compile(&self, cfg: &WasmPluginShellConfig) -> Result<Arc<Module>, WasmHostError> {
        let key = module_cache_key(cfg);
        if cfg.use_cache {
            if let Some(m) = self.inner.get(&key) {
                return Ok(m);
            }
        }
        let bytes = fetch_wasm_bytes_sync_with_auth(cfg.url.trim(), cfg.oci_auth.as_ref())?;
        verify_sha256(&bytes, cfg.sha256.as_deref())?;
        let m = Arc::new(Module::new(self.engine, &bytes)?);
        if cfg.use_cache {
            self.inner.insert(key, m.clone());
        }
        Ok(m)
    }
}

fn module_cache_key(cfg: &WasmPluginShellConfig) -> String {
    let mut key = cfg.module_cache_key.as_deref().filter(|s| !s.trim().is_empty()).unwrap_or_else(|| cfg.url.trim()).to_string();
    if let Some(sha256) = cfg.sha256.as_deref().filter(|s| !s.trim().is_empty()) {
        key.push_str("#sha256=");
        key.push_str(normalize_sha256(sha256));
    }
    key
}

fn normalize_sha256(s: &str) -> &str {
    s.trim().strip_prefix("sha256:").unwrap_or_else(|| s.trim())
}

fn verify_sha256(bytes: &[u8], expected: Option<&str>) -> Result<(), WasmHostError> {
    let Some(expected) = expected.map(normalize_sha256).filter(|s| !s.is_empty()) else {
        return Ok(());
    };
    let actual = format!("{:x}", Sha256::digest(bytes));
    if actual.eq_ignore_ascii_case(expected) {
        Ok(())
    } else {
        Err(WasmHostError::Fetch(format!("sha256 mismatch: expected {expected}, actual {actual}",)))
    }
}

static CACHE: OnceCell<WasmModuleCache> = OnceCell::new();

/// 默认缓存（容量 64）；多实例同 URL 共享编译结果。
pub fn default_module_cache() -> &'static WasmModuleCache {
    CACHE.get_or_init(|| WasmModuleCache::new(64))
}
