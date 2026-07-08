use k8s_openapi::schemars::JsonSchema;
use kube::CustomResource;
use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(CustomResource, Deserialize, Serialize, Clone, Debug, JsonSchema)]
#[serde(rename_all = "camelCase")]
#[kube(kind = "WasmPlugin", group = "extensions.higress.io", version = "v1alpha1", namespaced, status = "HigressWasmPluginStatus")]
pub struct HigressWasmPluginSpec {
    /// Higress-compatible wasm URL. Spacegate runtime supports local paths,
    /// `file://`, `http(s)://`, and OCI image URLs such as `oci://registry/repo:tag`.
    pub url: String,
    /// Optional plugin name exposed to proxy-wasm guests.
    #[serde(default)]
    pub plugin_name: Option<String>,
    /// Optional SHA-256 digest for the wasm bytes. Accepts either plain hex or `sha256:<hex>`.
    #[serde(default, alias = "sha256")]
    pub sha256: Option<String>,
    /// Higress phase is kept for ordering/compatibility. Spacegate currently maps order by priority.
    #[serde(default)]
    pub phase: Option<String>,
    /// Higher priority plugins are placed earlier in the generated Spacegate plugin list.
    #[serde(default)]
    pub priority: Option<i32>,
    /// `Always` disables Spacegate's in-process wasm module cache for this plugin.
    #[serde(default)]
    pub image_pull_policy: Option<String>,
    /// Optional Kubernetes Secret used for private OCI registries.
    #[serde(default)]
    pub image_pull_secret: Option<String>,
    /// Disable global/default config. Match rules can still enable per-rule configs.
    #[serde(default)]
    pub default_config_disable: bool,
    /// Higress default plugin config.
    #[serde(default)]
    pub default_config: Value,
    /// Optional match rules. These are passed through to Higress-style wasm plugins under `_rules_`.
    #[serde(default)]
    pub match_rules: Vec<HigressWasmPluginMatchRule>,
    /// Optional fail strategy. `FAIL_OPEN`/`FAIL_CLOSE` and `fail_open`/`fail_close` are accepted.
    #[serde(default)]
    pub fail_strategy: Option<String>,
}

#[derive(Deserialize, Serialize, Clone, Debug, Default, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct HigressWasmPluginStatus {
    #[serde(default)]
    pub observed_generation: Option<i64>,
    #[serde(default)]
    pub phase: Option<String>,
    #[serde(default)]
    pub digest: Option<String>,
    #[serde(default)]
    pub message: Option<String>,
}

#[derive(Deserialize, Serialize, Clone, Debug, Default, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct HigressWasmPluginMatchRule {
    #[serde(default)]
    pub ingress: Vec<String>,
    #[serde(default)]
    pub domain: Vec<String>,
    #[serde(default)]
    pub service: Vec<String>,
    #[serde(default)]
    pub config_disable: bool,
    #[serde(default)]
    pub config: Value,
}
