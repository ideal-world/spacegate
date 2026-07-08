//! WASM 插件宿主侧错误类型（实现 `std::error::Error`，可自动装箱为 `BoxError`）。

#[derive(Debug, thiserror::Error)]
pub enum WasmHostError {
    #[error("fetch wasm: {0}")]
    Fetch(String),
    #[error("wasmtime: {0}")]
    Wasmtime(#[from] wasmtime::Error),
    #[error("instantiation failed: {0}")]
    Instantiate(String),
    #[error("guest abi violation: {0}")]
    AbiViolation(String),
    #[error("memory oob: ptr={ptr} len={len}")]
    MemoryOob { ptr: u32, len: u32 },
    #[error("wasm guest trap during {hook}: {source}")]
    GuestTrap { hook: &'static str, source: wasmtime::Error },
    #[error("dispatch_http_call: {0}")]
    Dispatch(String),
    #[error("body too large: {actual} bytes exceeds limit {limit} bytes")]
    BodyTooLarge { actual: usize, limit: usize },
    #[error("resource limit: {0}")]
    ResourceLimit(String),
    #[error("config: {0}")]
    Config(String),
}

impl WasmHostError {
    pub fn requires_vm_rebuild(&self) -> bool {
        matches!(self, Self::GuestTrap { .. } | Self::Wasmtime(_) | Self::Dispatch(_) | Self::ResourceLimit(_))
    }
}
