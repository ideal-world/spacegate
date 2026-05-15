use kube::ResourceExt;
use serde_json::{json, Map, Value};
use spacegate_model::{
    ext::k8s::crd::wasm_plugin::{HigressWasmPluginMatchRule, WasmPlugin},
    BackendHost, PluginConfig, PluginInstanceId, PluginInstanceName, SgBackendRef,
};

const WASM_CODE: &str = "wasm";

pub(crate) trait HigressWasmPluginConv {
    fn to_spacegate_plugin_id(&self) -> PluginInstanceId;
    fn to_spacegate_rule_plugin_id(&self, rule_index: usize) -> PluginInstanceId;
    fn to_spacegate_plugin_configs(&self) -> Vec<PluginConfig>;
    fn to_spacegate_plugin_configs_with_oci_auth(&self, oci_auth: Option<Value>) -> Vec<PluginConfig>;
    fn to_spacegate_plugin_config_by_id(&self, id: &PluginInstanceId) -> Option<PluginConfig>;
    fn to_spacegate_plugin_config_by_id_with_oci_auth(&self, id: &PluginInstanceId, oci_auth: Option<Value>) -> Option<PluginConfig>;
    fn gateway_plugin_id(&self) -> Option<PluginInstanceId>;
    fn route_plugin_ids(&self, route_name: &str, hostnames: Option<&[String]>) -> Vec<PluginInstanceId>;
    fn backend_plugin_ids<P>(&self, backend: &SgBackendRef<P>) -> Vec<PluginInstanceId>;
    fn priority(&self) -> i32;
    fn phase_rank(&self) -> i32;
    fn validate_for_spacegate(&self) -> Result<(), String>;
    fn digest(&self) -> Option<String>;
    fn oci_registry(&self) -> Option<String>;
}

impl HigressWasmPluginConv for WasmPlugin {
    fn to_spacegate_plugin_id(&self) -> PluginInstanceId {
        PluginInstanceId {
            code: WASM_CODE.into(),
            name: PluginInstanceName::named(format!("higress-{}", self.name_any())),
        }
    }

    fn to_spacegate_rule_plugin_id(&self, rule_index: usize) -> PluginInstanceId {
        PluginInstanceId {
            code: WASM_CODE.into(),
            name: PluginInstanceName::named(format!("higress-{}-rule-{rule_index}", self.name_any())),
        }
    }

    fn to_spacegate_plugin_configs(&self) -> Vec<PluginConfig> {
        self.to_spacegate_plugin_configs_with_oci_auth(None)
    }

    fn to_spacegate_plugin_configs_with_oci_auth(&self, oci_auth: Option<Value>) -> Vec<PluginConfig> {
        let mut configs = Vec::new();
        if !self.spec.default_config_disable {
            configs.push(build_plugin_config(
                self,
                self.to_spacegate_plugin_id(),
                self.spec.default_config.clone(),
                "default",
                oci_auth.clone(),
            ));
        }
        configs.extend(self.spec.match_rules.iter().enumerate().filter(|(_, rule)| !rule.config_disable).map(|(idx, rule)| {
            build_plugin_config(
                self,
                self.to_spacegate_rule_plugin_id(idx),
                build_higress_rule_config(rule),
                &format!("rule-{idx}"),
                oci_auth.clone(),
            )
        }));
        configs
    }

    fn to_spacegate_plugin_config_by_id(&self, id: &PluginInstanceId) -> Option<PluginConfig> {
        self.to_spacegate_plugin_config_by_id_with_oci_auth(id, None)
    }

    fn to_spacegate_plugin_config_by_id_with_oci_auth(&self, id: &PluginInstanceId, oci_auth: Option<Value>) -> Option<PluginConfig> {
        self.to_spacegate_plugin_configs_with_oci_auth(oci_auth).into_iter().find(|cfg| &cfg.id == id)
    }

    fn gateway_plugin_id(&self) -> Option<PluginInstanceId> {
        (!self.spec.default_config_disable).then(|| self.to_spacegate_plugin_id())
    }

    fn route_plugin_ids(&self, route_name: &str, hostnames: Option<&[String]>) -> Vec<PluginInstanceId> {
        self.spec
            .match_rules
            .iter()
            .enumerate()
            .filter(|(_, rule)| !rule.config_disable && rule_matches_route(rule, route_name, hostnames))
            .map(|(idx, _)| self.to_spacegate_rule_plugin_id(idx))
            .collect()
    }

    fn backend_plugin_ids<P>(&self, backend: &SgBackendRef<P>) -> Vec<PluginInstanceId> {
        self.spec
            .match_rules
            .iter()
            .enumerate()
            .filter(|(_, rule)| !rule.config_disable && rule_matches_backend(rule, backend))
            .map(|(idx, _)| self.to_spacegate_rule_plugin_id(idx))
            .collect()
    }

    fn priority(&self) -> i32 {
        self.spec.priority.unwrap_or(0)
    }

    fn phase_rank(&self) -> i32 {
        phase_rank(self.spec.phase.as_deref())
    }

    fn validate_for_spacegate(&self) -> Result<(), String> {
        let url = self.spec.url.trim();
        if url.is_empty() {
            return Err("spec.url is empty".to_string());
        }
        if is_oci_url(url) && parse_oci_registry(url).is_none() {
            return Err("spec.url must include OCI registry and repository".to_string());
        }
        Ok(())
    }

    fn digest(&self) -> Option<String> {
        self.spec.sha256.clone()
    }

    fn oci_registry(&self) -> Option<String> {
        parse_oci_registry(&self.spec.url)
    }
}

fn build_plugin_config(plugin: &WasmPlugin, id: PluginInstanceId, plugin_config: Value, instance_suffix: &str, oci_auth: Option<Value>) -> PluginConfig {
    let namespace = plugin.namespace().unwrap_or_else(|| "default".to_string());
    let resource_version = plugin.resource_version().unwrap_or_else(|| "unknown".to_string());
    let plugin_name = plugin.spec.plugin_name.clone().unwrap_or_else(|| plugin.name_any());
    let image_pull_always = plugin.spec.image_pull_policy.as_deref().map(|v| v.eq_ignore_ascii_case("always")).unwrap_or(false);

    let mut spec = json!({
        "url": plugin.spec.url,
        "plugin_config": plugin_config,
        "plugin_name": plugin_name,
        "plugin_root_id": format!("higress-{}-root-{instance_suffix}", plugin.name_any()),
        "plugin_vm_id": format!("higress-{}-{}-{instance_suffix}", namespace, plugin.name_any()),
        "module_cache_key": format!("higress-wasmplugin:{namespace}:{}:{resource_version}:{instance_suffix}", plugin.name_any()),
        "use_cache": !image_pull_always,
    });

    if let Some(sha256) = plugin.spec.sha256.as_deref().filter(|v| !v.trim().is_empty()) {
        spec["sha256"] = Value::String(sha256.to_string());
    }
    if let Some(fail_strategy) = plugin.spec.fail_strategy.as_deref().and_then(normalize_fail_strategy) {
        spec["fail_strategy"] = Value::String(fail_strategy.to_string());
    }
    if let Some(oci_auth) = oci_auth {
        spec["oci_auth"] = oci_auth;
    }

    PluginConfig { id, spec }
}

pub(crate) fn sort_higress_wasm_plugins(plugins: &mut [WasmPlugin]) {
    plugins.sort_by(|a, b| a.phase_rank().cmp(&b.phase_rank()).then_with(|| b.priority().cmp(&a.priority())).then_with(|| a.name_any().cmp(&b.name_any())));
}

fn normalize_fail_strategy(value: &str) -> Option<&'static str> {
    match value.trim().to_ascii_lowercase().replace('-', "_").as_str() {
        "fail_open" | "failopen" => Some("fail_open"),
        "fail_close" | "failclose" => Some("fail_close"),
        _ => None,
    }
}

fn build_higress_rule_config(rule: &HigressWasmPluginMatchRule) -> Value {
    let mut config = value_to_object(rule.config.clone());
    if !rule.ingress.is_empty() {
        config.insert("_match_route_".to_string(), strings_value(rule.ingress.clone()));
    }
    if !rule.domain.is_empty() {
        config.insert("_match_domain_".to_string(), strings_value(rule.domain.clone()));
    }
    if !rule.service.is_empty() {
        config.insert("_match_service_".to_string(), strings_value(rule.service.clone()));
    }
    if rule.config_disable {
        config.insert("_config_disable_".to_string(), Value::Bool(true));
    }
    Value::Object(config)
}

fn value_to_object(value: Value) -> Map<String, Value> {
    match value {
        Value::Object(map) => map,
        Value::Null => Map::new(),
        other => {
            let mut map = Map::new();
            map.insert("_config_".to_string(), other);
            map
        }
    }
}

fn strings_value(values: Vec<String>) -> Value {
    Value::Array(values.into_iter().map(Value::String).collect())
}

fn phase_rank(phase: Option<&str>) -> i32 {
    match phase.unwrap_or_default().trim().to_ascii_uppercase().as_str() {
        "AUTHN" => 10,
        "AUTHZ" => 20,
        "STATS" => 90,
        _ => 50,
    }
}

fn rule_matches_route(rule: &HigressWasmPluginMatchRule, route_name: &str, hostnames: Option<&[String]>) -> bool {
    let route_match = !rule.ingress.is_empty() && rule.ingress.iter().any(|name| name.eq_ignore_ascii_case(route_name));
    let domain_match =
        !rule.domain.is_empty() && hostnames.map(|hostnames| hostnames.iter().any(|hostname| rule.domain.iter().any(|domain| domain_matches(domain, hostname)))).unwrap_or(false);
    let rule_has_no_explicit_target = rule.ingress.is_empty() && rule.domain.is_empty() && rule.service.is_empty();
    route_match || domain_match || rule_has_no_explicit_target
}

fn rule_matches_backend<P>(rule: &HigressWasmPluginMatchRule, backend: &SgBackendRef<P>) -> bool {
    !rule.service.is_empty() && rule.service.iter().any(|service| backend_matches_service(backend, service))
}

fn domain_matches(pattern: &str, hostname: &str) -> bool {
    let pattern = pattern.trim().trim_end_matches('.');
    let hostname = hostname.trim().trim_end_matches('.');
    if pattern == "*" {
        return true;
    }
    if let Some(suffix) = pattern.strip_prefix("*.") {
        return hostname.eq_ignore_ascii_case(suffix) || hostname.to_ascii_lowercase().ends_with(&format!(".{}", suffix.to_ascii_lowercase()));
    }
    pattern.eq_ignore_ascii_case(hostname)
}

fn backend_matches_service<P>(backend: &SgBackendRef<P>, service: &str) -> bool {
    let service = service.trim();
    match &backend.host {
        BackendHost::K8sService(data) => {
            data.name.eq_ignore_ascii_case(service) || data.namespace.as_ref().map(|ns| format!("{}.{}", data.name, ns).eq_ignore_ascii_case(service)).unwrap_or(false)
        }
        _ => backend.get_host().eq_ignore_ascii_case(service),
    }
}

fn is_oci_url(url: &str) -> bool {
    let lower = url.to_ascii_lowercase();
    lower.starts_with("oci://") || lower.starts_with("docker://") || lower.starts_with("image://") || lower.starts_with("oci+http://")
}

fn parse_oci_registry(url: &str) -> Option<String> {
    let trim = url.trim();
    let rest = trim.strip_prefix("oci://").or_else(|| trim.strip_prefix("docker://")).or_else(|| trim.strip_prefix("image://")).or_else(|| trim.strip_prefix("oci+http://"))?;
    let (registry, repository) = rest.split_once('/')?;
    (!registry.trim().is_empty() && !repository.trim().is_empty()).then(|| registry.to_string())
}

#[cfg(test)]
mod tests {
    use k8s_openapi::apimachinery::pkg::apis::meta::v1::ObjectMeta;
    use serde_json::json;
    use spacegate_model::{
        ext::k8s::crd::wasm_plugin::{HigressWasmPluginSpec, WasmPlugin},
        BackendHost, K8sServiceData, SgBackendRef,
    };

    use super::*;

    #[test]
    fn converts_higress_wasmplugin_to_spacegate_wasm_plugin_config() {
        let plugin = WasmPlugin {
            metadata: ObjectMeta {
                name: Some("authn".to_string()),
                namespace: Some("gw".to_string()),
                resource_version: Some("42".to_string()),
                ..Default::default()
            },
            spec: HigressWasmPluginSpec {
                url: "https://example.com/authn.wasm".to_string(),
                plugin_name: Some("authn-plugin".to_string()),
                sha256: Some("sha256:abc".to_string()),
                phase: Some("AUTHN".to_string()),
                priority: Some(100),
                image_pull_policy: Some("IfNotPresent".to_string()),
                image_pull_secret: None,
                default_config_disable: false,
                default_config: json!({"issuer": "spacegate"}),
                match_rules: vec![HigressWasmPluginMatchRule {
                    ingress: vec!["api-route".to_string()],
                    domain: vec!["api.example.com".to_string()],
                    service: vec![],
                    config_disable: false,
                    config: json!({"issuer": "route"}),
                }],
                fail_strategy: Some("FAIL_CLOSE".to_string()),
            },
            status: None,
        };

        let cfg = plugin.to_spacegate_plugin_configs().into_iter().next().expect("default config");
        assert_eq!(cfg.id.to_string(), "wasm-n-higress-authn");
        assert_eq!(cfg.spec["url"], "https://example.com/authn.wasm");
        assert_eq!(cfg.spec["sha256"], "sha256:abc");
        assert_eq!(cfg.spec["fail_strategy"], "fail_close");
        assert_eq!(cfg.spec["module_cache_key"], "higress-wasmplugin:gw:authn:42:default");
        assert_eq!(cfg.spec["plugin_config"]["issuer"], "spacegate");

        let rule_cfg = plugin.to_spacegate_plugin_config_by_id(&plugin.to_spacegate_rule_plugin_id(0)).expect("rule config");
        assert_eq!(rule_cfg.id.to_string(), "wasm-n-higress-authn-rule-0");
        assert_eq!(rule_cfg.spec["plugin_config"]["issuer"], "route");
        assert_eq!(rule_cfg.spec["plugin_config"]["_match_route_"][0], "api-route");
        assert_eq!(rule_cfg.spec["plugin_config"]["_match_domain_"][0], "api.example.com");
    }

    #[test]
    fn rule_ids_match_routes_domains_and_services() {
        let plugin = WasmPlugin {
            metadata: ObjectMeta {
                name: Some("ratelimit".to_string()),
                namespace: Some("gw".to_string()),
                resource_version: Some("7".to_string()),
                ..Default::default()
            },
            spec: HigressWasmPluginSpec {
                url: "file:///tmp/ratelimit.wasm".to_string(),
                plugin_name: None,
                sha256: None,
                phase: Some("AUTHZ".to_string()),
                priority: Some(10),
                image_pull_policy: None,
                image_pull_secret: None,
                default_config_disable: true,
                default_config: serde_json::Value::Null,
                match_rules: vec![
                    HigressWasmPluginMatchRule {
                        ingress: vec!["api-route".to_string()],
                        domain: vec![],
                        service: vec![],
                        config_disable: false,
                        config: json!({"limit": 10}),
                    },
                    HigressWasmPluginMatchRule {
                        ingress: vec![],
                        domain: vec!["*.example.com".to_string()],
                        service: vec![],
                        config_disable: false,
                        config: json!({"limit": 20}),
                    },
                    HigressWasmPluginMatchRule {
                        ingress: vec![],
                        domain: vec![],
                        service: vec!["backend.default".to_string()],
                        config_disable: false,
                        config: json!({"limit": 30}),
                    },
                ],
                fail_strategy: None,
            },
            status: None,
        };

        let route_ids = plugin.route_plugin_ids("api-route", Some(&["shop.example.com".to_string()]));
        assert_eq!(
            route_ids.iter().map(ToString::to_string).collect::<Vec<_>>(),
            vec!["wasm-n-higress-ratelimit-rule-0", "wasm-n-higress-ratelimit-rule-1"]
        );

        let backend = SgBackendRef::<PluginInstanceId> {
            host: BackendHost::K8sService(K8sServiceData {
                name: "backend".to_string(),
                namespace: Some("default".to_string()),
            }),
            ..Default::default()
        };
        let backend_ids = plugin.backend_plugin_ids(&backend);
        assert_eq!(backend_ids.iter().map(ToString::to_string).collect::<Vec<_>>(), vec!["wasm-n-higress-ratelimit-rule-2"]);
    }

    #[test]
    fn validates_oci_urls_for_status() {
        let plugin = WasmPlugin {
            metadata: ObjectMeta {
                name: Some("oci-plugin".to_string()),
                namespace: Some("gw".to_string()),
                resource_version: Some("1".to_string()),
                ..Default::default()
            },
            spec: HigressWasmPluginSpec {
                url: "oci://registry.example.com/plugin:v1".to_string(),
                plugin_name: None,
                sha256: None,
                phase: None,
                priority: None,
                image_pull_policy: Some("Always".to_string()),
                image_pull_secret: Some("pull-secret".to_string()),
                default_config_disable: false,
                default_config: serde_json::Value::Null,
                match_rules: vec![],
                fail_strategy: None,
            },
            status: None,
        };

        plugin.validate_for_spacegate().expect("OCI should be accepted");
        assert_eq!(plugin.oci_registry().as_deref(), Some("registry.example.com"));
    }
}
