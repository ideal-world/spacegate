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
    #[serde(default)]
    pub max_memory_pages: Option<u32>,
    #[serde(default)]
    pub fuel_per_call: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct WasmPluginShellConfig {
    /// `file://`、`http(s)://` 或本地路径。
    pub url: String,
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
            plugin_config: serde_json::Value::Null,
            fail_strategy: FailStrategy::FailOpen,
            clusters: HashMap::new(),
            limits: WasmLimits::default(),
            validate_on_create: false,
            plugin_name: String::new(),
            plugin_root_id: String::new(),
            plugin_vm_id: default_vm_id(),
        }
    }
}

impl WasmPluginShellConfig {
    /// 把 `plugin_config`（任意 JSON）转换为 hai 风格 YAML 字节流。
    ///
    /// hai-process-mix 在 `on_configure` 内是 `serde_yaml::from_slice::<PluginConfig>(&bytes)`，
    /// 所以无论上层用 JSON 还是 YAML 写，传给 guest 的都必须是 YAML 序列化结果。
    pub fn configuration_bytes(&self) -> Vec<u8> {
        if self.plugin_config.is_null() {
            return Vec::new();
        }
        serde_yaml::to_string(&self.plugin_config)
            .unwrap_or_default()
            .into_bytes()
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
