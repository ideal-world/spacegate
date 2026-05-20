//! `WasmPluginShell` 的 JSON spec（与演进文档 §5 对齐）。

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, Default, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum FailStrategy {
    #[default]
    FailOpen,
    FailClose,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct WasmLimits {
    /// 单个 VM 的线性内存页数上限（1 page = 64KiB）。
    #[serde(default)]
    pub max_memory_pages: Option<u32>,
    /// 每次 guest hook 调用前补充的 fuel；默认不配置时使用近似无限预算。
    #[serde(default)]
    pub fuel_per_call: Option<u64>,
    /// 每次 guest hook 的 epoch 超时窗口，单位毫秒；依赖 host 的 1ms epoch ticker。
    #[serde(default)]
    pub epoch_timeout_millis: Option<u64>,
    /// host 需要物化 body 时允许的最大字节数，覆盖请求 body、响应 body、dispatch 请求/响应 body。
    #[serde(default)]
    pub max_body_bytes: Option<usize>,
    /// 单个 VM 同时允许的未完成 `proxy_http_call` 数量。
    #[serde(default)]
    pub max_pending_calls: Option<usize>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct OciAuthConfig {
    /// Optional registry hint, for example `registry.cn-hangzhou.aliyuncs.com`.
    #[serde(default)]
    pub registry: Option<String>,
    /// Basic-auth username used for registry token exchange or direct registry auth.
    #[serde(default)]
    pub username: Option<String>,
    /// Basic-auth password used for registry token exchange or direct registry auth.
    #[serde(default)]
    pub password: Option<String>,
    /// Pre-issued bearer token for registries that do not need a token challenge exchange.
    #[serde(default)]
    pub bearer_token: Option<String>,
    /// Docker config `identitytoken`; treated as a bearer token by the registry client.
    #[serde(default)]
    pub identity_token: Option<String>,
}

fn default_use_cache() -> bool {
    true
}

fn default_vm_pool_size() -> usize {
    1
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct WasmPluginShellConfig {
    /// `file://`、`http(s)://`、OCI 镜像 URL 或本地路径。
    pub url: String,
    /// Optional OCI registry auth. Usually populated from Higress `imagePullSecret`.
    #[serde(default)]
    pub oci_auth: Option<OciAuthConfig>,
    /// 可选 SHA-256 校验值，支持裸 hex 或 `sha256:<hex>`。
    ///
    /// 配置该字段后，host 会在编译前校验拉取到的 wasm 字节；字段变化也会自动让模块缓存失效。
    #[serde(default)]
    pub sha256: Option<String>,
    /// 可选模块缓存键。
    ///
    /// 默认按 `url` 加 `sha256` 复用编译产物；当远端同 URL 发布新版本且未配置 sha256 时，
    /// 可以把这里设置成版本号/etag/digest 来强制重新拉取并编译。
    #[serde(default)]
    pub module_cache_key: Option<String>,
    /// 是否复用进程内 wasm Module 缓存。
    ///
    /// 默认开启；关闭后每次创建/更新插件实例都会重新拉取并编译，适合开发调试。
    #[serde(default = "default_use_cache")]
    pub use_cache: bool,
    /// 传给 guest `proxy_on_configure` 的配置：可为 JSON 对象;序列化为 YAML 字节给 hai 系插件。
    #[serde(default)]
    pub plugin_config: serde_json::Value,
    #[serde(default)]
    pub fail_strategy: FailStrategy,
    /// `dispatch_http_call` 时 guest 传入的 cluster 名 → 真实 HTTP base URL。
    ///
    /// 兼容 hai 的 Higress cluster 写法 `outbound|<port>||<host>.<ns>`：
    /// 若直接命中则用配置 base，否则 host 会回退用 `:authority` header（hai 已带）发起请求。
    #[serde(default)]
    pub clusters: HashMap<String, String>,
    #[serde(default)]
    pub limits: WasmLimits,
    /// 创建时是否尝试用占位 linker 实例化一次（尽早发现链接错误）。当前实现已弃用，保留兼容字段。
    #[serde(default = "default_validate")]
    pub validate_on_create: bool,
    /// 暴露给 guest 的 `plugin_name` well-known property（spec §Properties §Proxy-Wasm properties）。
    #[serde(default)]
    pub plugin_name: String,
    /// 暴露给 guest 的 `plugin_root_id` well-known property。
    #[serde(default)]
    pub plugin_root_id: String,
    /// 暴露给 guest 的 `plugin_vm_id` well-known property；同时用于 `proxy_resolve_shared_queue`。
    #[serde(default = "default_vm_id")]
    pub plugin_vm_id: String,
    /// 同一个 wasm 插件实例内创建的 VM 数量。
    ///
    /// 默认 1，保持单 VM 串行语义；设置为大于 1 后，多个独立 VM 共享同一个已编译 Module，
    /// 请求按 try-lock + round-robin 分发，用于降低长时间 `dispatch_http_call` 对后续请求的阻塞。
    #[serde(default = "default_vm_pool_size")]
    pub vm_pool_size: usize,
    /// wait 策略专用 VM 池大小。
    ///
    /// 默认 0，表示不启用分类调度，所有请求都进入普通 VM 池。设置为大于 0 后，
    /// 带 `X-RateLimit-Policy: wait` 的请求会进入独立 wait 池，避免长等待请求占满普通池。
    #[serde(default)]
    pub wait_vm_pool_size: usize,
}

fn default_vm_id() -> String {
    "default".to_string()
}

fn default_validate() -> bool {
    false
}

impl Default for WasmPluginShellConfig {
    fn default() -> Self {
        Self {
            url: String::new(),
            oci_auth: None,
            sha256: None,
            module_cache_key: None,
            use_cache: default_use_cache(),
            plugin_config: serde_json::Value::Null,
            fail_strategy: FailStrategy::FailOpen,
            clusters: HashMap::new(),
            limits: WasmLimits::default(),
            validate_on_create: false,
            plugin_name: String::new(),
            plugin_root_id: String::new(),
            plugin_vm_id: default_vm_id(),
            vm_pool_size: default_vm_pool_size(),
            wait_vm_pool_size: 0,
        }
    }
}

impl WasmPluginShellConfig {
    pub fn normalized_vm_pool_size(&self) -> usize {
        self.vm_pool_size.clamp(1, 64)
    }

    pub fn normalized_wait_vm_pool_size(&self) -> usize {
        self.wait_vm_pool_size.min(64)
    }

    pub fn max_memory_bytes(&self) -> Option<usize> {
        self.limits.max_memory_pages.map(|pages| pages as usize * 64 * 1024)
    }

    pub fn guest_fuel_per_call(&self) -> u64 {
        self.limits.fuel_per_call.unwrap_or(u64::MAX / 4).max(1)
    }

    pub fn guest_epoch_deadline_ticks(&self) -> u64 {
        // epoch ticker 以 1ms 为一跳；默认给一个很大的窗口，相当于不主动超时。
        self.limits.epoch_timeout_millis.unwrap_or(24 * 60 * 60 * 1000).clamp(1, 24 * 60 * 60 * 1000)
    }

    /// 把 `plugin_config`（任意 JSON）转换为 hai 风格 YAML 字节流。
    ///
    /// hai-process-mix 在 `on_configure` 内是 `serde_yaml::from_slice::<PluginConfig>(&bytes)`，
    /// 所以无论上层用 JSON 还是 YAML 写，传给 guest 的都必须是 YAML 序列化结果。
    pub fn configuration_bytes(&self) -> Vec<u8> {
        if self.plugin_config.is_null() {
            return Vec::new();
        }
        serde_yaml::to_string(&self.plugin_config).unwrap_or_default().into_bytes()
    }

    /// 给定 guest 传来的 cluster 字符串，返回基础 URL（`http://host:port`）。
    ///
    /// 优先精确匹配配置 map；其次尝试解析 Envoy/Higress 习惯写法
    /// `outbound|<port>||<host>` -> `http://<host>:<port>`；
    /// 都不命中返回 `None`。
    pub fn resolve_cluster(&self, cluster: &str) -> Option<String> {
        if let Some(v) = self.clusters.get(cluster) {
            return Some(v.clone());
        }
        if let Some(rest) = cluster.strip_prefix("outbound|") {
            let mut parts = rest.splitn(2, "||");
            let port = parts.next()?.trim();
            let host = parts.next()?.trim();
            if host.is_empty() || port.is_empty() {
                return None;
            }
            return Some(format!("http://{host}:{port}"));
        }
        None
    }
}
