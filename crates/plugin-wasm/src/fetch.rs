//! 同步拉取 WASM 字节（在 `Plugin::create` 同步上下文中使用）。
//!
//! 支持：`file://...` 与裸文件系统路径；`http(s)://...` 暂未在 reqwest blocking 下启用，
//! 后续按 OCI 接入时一起做。

use crate::error::WasmHostError;

pub fn fetch_wasm_bytes_sync(url_or_path: &str) -> Result<Vec<u8>, WasmHostError> {
    let trim = url_or_path.trim();
    if let Some(rest) = trim.strip_prefix("file://") {
        return std::fs::read(rest).map_err(|e| WasmHostError::Fetch(format!("read file {rest}: {e}")));
    }
    if trim.starts_with("http://") || trim.starts_with("https://") {
        return Err(WasmHostError::Fetch(
            "http(s)://wasm 拉取暂未启用：请使用 file:// 或裸路径".to_string(),
        ));
    }
    std::fs::read(trim).map_err(|e| WasmHostError::Fetch(format!("read path {trim}: {e}")))
}
