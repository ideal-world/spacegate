use base64::{engine::general_purpose, Engine as _};
use futures_util::future::join_all;
use gateway::{SgListener, SgParameters, SgProtocolConfig, SgTlsConfig};
use http_route::SgHttpRouteRule;
use k8s_gateway_api::{Gateway, HttpRoute, Listener};
use k8s_openapi::api::core::v1::Secret;
use kube::{api::ListParams, Api, ResourceExt};
use serde_json::{json, Value};
use spacegate_model::{
    ext::k8s::{
        crd::{
            http_spaceroute::HttpSpaceroute,
            sg_filter::{K8sSgFilterSpecTargetRef, SgFilter},
            wasm_plugin::WasmPlugin,
        },
        helper_struct::SgTargetKind,
    },
    PluginInstanceId,
};

use crate::{
    constants::{self, GATEWAY_CLASS_NAME},
    model::{gateway, http_route, PluginConfig, SgGateway, SgHttpRoute},
    service::Retrieve,
    BoxError, BoxResult,
};

use super::{
    convert::{
        filter_k8s_conv::PluginConfigConv,
        gateway_k8s_conv::SgParametersConv as _,
        higress_wasm_plugin_conv::{sort_higress_wasm_plugins, HigressWasmPluginConv as _},
        route_k8s_conv::SgHttpRouteRuleConv as _,
    },
    K8s,
};

impl Retrieve for K8s {
    async fn retrieve_config_item_gateway(&self, gateway_name: &str) -> BoxResult<Option<SgGateway>> {
        let gateway_api: Api<Gateway> = self.get_namespace_api();

        let result = if let Some(gateway_obj) = gateway_api.get_opt(gateway_name).await?.and_then(|gateway_obj| {
            if gateway_obj.spec.gateway_class_name == GATEWAY_CLASS_NAME {
                Some(gateway_obj)
            } else {
                None
            }
        }) {
            Some(self.kube_gateway_2_sg_gateway(gateway_obj).await?)
        } else {
            None
        };

        Ok(result)
    }

    async fn retrieve_config_item_route(&self, gateway_name: &str, route_name: &str) -> BoxResult<Option<SgHttpRoute>> {
        let http_spaceroute_api: Api<HttpSpaceroute> = self.get_namespace_api();
        let httproute_api: Api<HttpRoute> = self.get_namespace_api();

        let result = if let Some(httpspaceroute) = http_spaceroute_api.get_opt(route_name).await?.and_then(|http_route_obj| {
            if http_route_obj
                .spec
                .inner
                .parent_refs
                .as_ref()
                .map(|parent_refs| parent_refs.iter().any(|parent_ref| parent_ref.namespace == http_route_obj.namespace() && parent_ref.name == gateway_name))
                .unwrap_or(false)
            {
                Some(http_route_obj)
            } else {
                None
            }
        }) {
            Some(self.kube_httpspaceroute_2_sg_route(httpspaceroute).await?)
        } else if let Some(http_route) = httproute_api.get_opt(route_name).await?.and_then(|http_route| {
            if http_route
                .spec
                .inner
                .parent_refs
                .as_ref()
                .map(|parent_refs| parent_refs.iter().any(|parent_ref| parent_ref.namespace == http_route.namespace() && parent_ref.name == gateway_name))
                .unwrap_or(false)
            {
                Some(http_route)
            } else {
                None
            }
        }) {
            Some(self.kube_httproute_2_sg_route(http_route).await?)
        } else {
            None
        };

        Ok(result)
    }

    async fn retrieve_config_item_route_names(&self, name: &str) -> BoxResult<Vec<String>> {
        let http_spaceroute_api: Api<HttpSpaceroute> = self.get_namespace_api();
        let httproute_api: Api<HttpRoute> = self.get_namespace_api();

        let mut result: Vec<String> = http_spaceroute_api
            .list(&ListParams::default())
            .await?
            .iter()
            .filter(|route| {
                route
                    .spec
                    .inner
                    .parent_refs
                    .as_ref()
                    .map(|parent_refs| parent_refs.iter().any(|parent_ref| parent_ref.namespace == route.namespace() && parent_ref.name == name))
                    .unwrap_or(false)
            })
            .map(|route| route.name_any())
            .collect();

        result.extend(
            httproute_api
                .list(&ListParams::default())
                .await?
                .iter()
                .filter(|route| {
                    route
                        .spec
                        .inner
                        .parent_refs
                        .as_ref()
                        .map(|parent_refs| parent_refs.iter().any(|parent_ref| parent_ref.namespace == route.namespace() && parent_ref.name == name))
                        .unwrap_or(false)
                })
                .map(|route| route.name_any()),
        );

        Ok(result)
    }

    async fn retrieve_config_names(&self) -> BoxResult<Vec<String>> {
        let gateway_api: Api<Gateway> = self.get_namespace_api();

        let result = gateway_api.list(&ListParams::default()).await?.iter().map(|gateway| gateway.name_any()).collect();

        Ok(result)
    }

    async fn retrieve_all_plugins(&self) -> Result<Vec<PluginConfig>, BoxError> {
        let filter_api: Api<SgFilter> = self.get_namespace_api();
        let wasm_plugin_api: Api<WasmPlugin> = self.get_namespace_api();

        let mut result = filter_api.list(&ListParams::default()).await?.into_iter().filter_map(PluginConfig::from_first_filter_obj).collect::<Vec<_>>();
        let mut wasm_plugins = wasm_plugin_api.list(&ListParams::default()).await?.items;
        sort_higress_wasm_plugins(&mut wasm_plugins);
        for plugin in wasm_plugins {
            let oci_auth = self.resolve_higress_wasm_oci_auth(&plugin).await?;
            if oci_auth.is_some() {
                result.extend(plugin.to_spacegate_plugin_configs_with_oci_auth(oci_auth));
            } else {
                result.extend(plugin.to_spacegate_plugin_configs());
            }
        }
        Ok(result)
    }

    async fn retrieve_plugin(&self, id: &spacegate_model::PluginInstanceId) -> Result<Option<PluginConfig>, BoxError> {
        let filter_api: Api<SgFilter> = self.get_namespace_api();
        let wasm_plugin_api: Api<WasmPlugin> = self.get_namespace_api();

        if id.code == "wasm" {
            if let spacegate_model::PluginInstanceName::Named { name } = &id.name {
                if let Some(wasm_name) = name.strip_prefix("higress-") {
                    let wasm_name = wasm_name.rsplit_once("-rule-").map(|(base, _)| base).unwrap_or(wasm_name);
                    if let Some(plugin) = wasm_plugin_api.get_opt(wasm_name).await? {
                        let oci_auth = self.resolve_higress_wasm_oci_auth(&plugin).await?;
                        return Ok(if oci_auth.is_some() {
                            plugin.to_spacegate_plugin_config_by_id_with_oci_auth(id, oci_auth)
                        } else {
                            plugin.to_spacegate_plugin_config_by_id(id)
                        });
                    }
                    return Ok(None);
                }
            }
        }
        match &id.name {
            spacegate_model::PluginInstanceName::Anon { uid: _ } => Ok(None),
            spacegate_model::PluginInstanceName::Named { name } => {
                let result = filter_api.get_opt(name).await?.and_then(PluginConfig::from_first_filter_obj);
                Ok(result)
            }
            spacegate_model::PluginInstanceName::Mono => Ok(None),
        }
    }

    async fn retrieve_plugins_by_code(&self, code: &str) -> Result<Vec<PluginConfig>, BoxError> {
        Ok(self.retrieve_all_plugins().await?.into_iter().filter(|p| p.code() == code).collect())
    }
}

impl K8s {
    pub(crate) const HTTP2_KEY: &'static str = "http2";
    pub(crate) const HTTP2_ENABLE: &'static str = "true";
    // query is http2 enabled?
    fn retrieve_http2_config(tls_config: &k8s_gateway_api::GatewayTlsConfig) -> bool {
        if let Some(options) = &tls_config.options {
            if let Some(Self::HTTP2_ENABLE) = options.get(Self::HTTP2_KEY).map(String::as_str) {
                return true;
            }
        }
        false
    }
    async fn kube_gateway_2_sg_gateway(&self, gateway_obj: Gateway) -> BoxResult<SgGateway> {
        let gateway_name = gateway_obj.name_any();
        let mut plugins = self
            .retrieve_config_item_filters(K8sSgFilterSpecTargetRef {
                kind: SgTargetKind::Gateway.into(),
                name: gateway_name.clone(),
                namespace: gateway_obj.namespace(),
            })
            .await?;
        plugins.extend(self.retrieve_higress_gateway_plugins(gateway_obj.namespace()).await?);
        let result = SgGateway {
            name: gateway_name,
            parameters: SgParameters::from_kube_gateway(&gateway_obj),
            listeners: self.retrieve_config_item_listeners(&gateway_obj.spec.listeners).await?,
            plugins,
        };
        Ok(result)
    }

    async fn kube_httpspaceroute_2_sg_route(&self, httpspace_route: HttpSpaceroute) -> BoxResult<SgHttpRoute> {
        let route_name = httpspace_route.name_any();
        let namespace = httpspace_route.namespace();
        let kind = if let Some(kind) = httpspace_route.annotations().get(constants::RAW_HTTP_ROUTE_KIND) {
            kind.clone()
        } else {
            SgTargetKind::Httpspaceroute.into()
        };
        let priority = httpspace_route.annotations().get(crate::constants::ANNOTATION_RESOURCE_PRIORITY).and_then(|a| a.parse::<i16>().ok()).unwrap_or(0);
        let plugins = self
            .retrieve_config_item_filters(K8sSgFilterSpecTargetRef {
                kind,
                name: route_name.clone(),
                namespace: httpspace_route.namespace(),
            })
            .await?;
        let mut route = SgHttpRoute {
            hostnames: httpspace_route.spec.hostnames.clone(),
            plugins,
            rules: httpspace_route
                .spec
                .rules
                .map(|r_vec| r_vec.into_iter().map(SgHttpRouteRule::from_kube_httproute).collect::<Result<Vec<_>, BoxError>>())
                .transpose()?
                .unwrap_or_default(),
            priority,
            route_name,
        };
        self.apply_higress_wasm_route_plugins(&mut route, namespace).await?;
        Ok(route)
    }

    async fn kube_httproute_2_sg_route(&self, http_route: HttpRoute) -> BoxResult<SgHttpRoute> {
        self.kube_httpspaceroute_2_sg_route(http_route.into()).await
    }

    async fn retrieve_config_item_filters(&self, target: K8sSgFilterSpecTargetRef) -> BoxResult<Vec<PluginInstanceId>> {
        let kind = target.kind;
        let name = target.name;
        let namespace = target.namespace.unwrap_or(self.namespace.to_string());

        let filter_api: Api<SgFilter> = self.get_all_api();
        let plugin_ids: Vec<PluginInstanceId> = filter_api
            .list(&ListParams::default())
            .await
            .map_err(Box::new)?
            .into_iter()
            .filter(|filter_obj| {
                filter_obj.spec.target_refs.iter().any(|target_ref| {
                    target_ref.kind.eq_ignore_ascii_case(&kind)
                        && target_ref.name.eq_ignore_ascii_case(&name)
                        && target_ref.namespace.as_deref().unwrap_or("default").eq_ignore_ascii_case(&namespace)
                })
            })
            .flat_map(|filter_obj| PluginConfig::from_first_filter_obj(filter_obj).map(|f| f.into()))
            .collect();
        let plugin_ids = plugin_ids;

        if !plugin_ids.is_empty() {
            let mut filter_vec = String::new();
            plugin_ids.clone().into_iter().for_each(|id| filter_vec.push_str(&format!("plugin:{{id:{}}},", id)));
            tracing::trace!("[SG.Common] {namespace}.{kind}.{name} filter found: {}", filter_vec.trim_end_matches(','));
        }

        if plugin_ids.is_empty() {
            Ok(vec![])
        } else {
            Ok(plugin_ids)
        }
    }

    async fn retrieve_config_item_listeners(&self, listeners: &[Listener]) -> BoxResult<Vec<SgListener>> {
        join_all(
            listeners
                .iter()
                .map(|listener| async move {
                    let sg_listener = SgListener {
                        name: listener.name.clone(),
                        ip: None,
                        port: listener.port,
                        protocol: match listener.protocol.to_lowercase().as_str() {
                            "http" => SgProtocolConfig::Http,
                            "https" => {
                                if let Some(tls_config) = &listener.tls {
                                    if let Some(certificate_ref) = tls_config.certificate_refs.as_ref().and_then(|vec| vec.first()) {
                                        let secret_api: Api<Secret> = self.get_namespace_api();
                                        if let Some(secret_obj) = secret_api.get_opt(&certificate_ref.name).await? {
                                            let tls = if let Some(secret_data) = secret_obj.data {
                                                if let Some(tls_crt) = secret_data.get("tls.crt") {
                                                    if let Some(tls_key) = secret_data.get("tls.key") {
                                                        Some(SgTlsConfig {
                                                            mode: tls_config.mode.clone().into(),
                                                            key: String::from_utf8(tls_key.0.clone()).expect("[SG.Config] Gateway tls secret [tls.key] is not valid utf8"),
                                                            cert: String::from_utf8(tls_crt.0.clone()).expect("[SG.Config] Gateway tls secret [tls.cert] is not valid utf8"),
                                                            http2: Some(Self::retrieve_http2_config(tls_config)),
                                                        })
                                                    } else {
                                                        tracing::warn!("[SG.Config] Gateway [spec.listener.protocol=https] tls.key is empty");
                                                        None
                                                    }
                                                } else {
                                                    tracing::warn!("[SG.Config] Gateway [spec.listener.protocol=https] tls.certificate_refs is empty");
                                                    None
                                                }
                                            } else {
                                                tracing::warn!("[SG.Config] Gateway [spec.listener.protocol=https] tls.data is empty");
                                                None
                                            };
                                            if let Some(tls) = tls {
                                                SgProtocolConfig::Https { tls }
                                            } else {
                                                SgProtocolConfig::Http
                                            }
                                        } else {
                                            tracing::warn!("[SG.Config] Gateway [spec.listener.protocol=https] tls.certificate_refs is empty");
                                            SgProtocolConfig::Http
                                        }
                                    } else {
                                        tracing::warn!("[SG.Config] Gateway [spec.listener.protocol=https] tls.certificate_refs is empty");
                                        SgProtocolConfig::Http
                                    }
                                } else {
                                    tracing::warn!("[SG.Config] Gateway [spec.listener.protocol=https] tls is empty");
                                    SgProtocolConfig::Http
                                }
                            }
                            _ => return Err("Unsupported protocol".into()),
                        },
                        hostname: listener.hostname.clone(),
                    };
                    Ok(sg_listener)
                })
                .collect::<Vec<_>>(),
        )
        .await
        .into_iter()
        .collect()
    }

    async fn retrieve_higress_gateway_plugins(&self, namespace: Option<String>) -> BoxResult<Vec<PluginInstanceId>> {
        let namespace = namespace.unwrap_or_else(|| self.namespace.to_string());
        let wasm_plugin_api: Api<WasmPlugin> = self.get_specify_namespace_api(&namespace);
        let mut wasm_plugins = wasm_plugin_api.list(&ListParams::default()).await?.items;
        sort_higress_wasm_plugins(&mut wasm_plugins);
        Ok(wasm_plugins.into_iter().filter_map(|p| p.gateway_plugin_id()).collect())
    }

    async fn apply_higress_wasm_route_plugins(&self, route: &mut SgHttpRoute, namespace: Option<String>) -> BoxResult<()> {
        let namespace = namespace.unwrap_or_else(|| self.namespace.to_string());
        let wasm_plugin_api: Api<WasmPlugin> = self.get_specify_namespace_api(&namespace);
        let mut wasm_plugins = wasm_plugin_api.list(&ListParams::default()).await?.items;
        sort_higress_wasm_plugins(&mut wasm_plugins);
        let hostnames = route.hostnames.as_deref();

        for plugin in wasm_plugins {
            route.plugins.extend(plugin.route_plugin_ids(&route.route_name, hostnames));
            for rule in &mut route.rules {
                for backend in &mut rule.backends {
                    backend.plugins.extend(plugin.backend_plugin_ids(backend));
                }
            }
        }
        Ok(())
    }

    async fn resolve_higress_wasm_oci_auth(&self, plugin: &WasmPlugin) -> BoxResult<Option<Value>> {
        if plugin.oci_registry().is_none() {
            return Ok(None);
        }
        let Some(secret_name) = plugin.spec.image_pull_secret.as_deref().map(str::trim).filter(|v| !v.is_empty()) else {
            return Ok(None);
        };
        let namespace = plugin.namespace().unwrap_or_else(|| self.namespace.to_string());
        let secret_api: Api<Secret> = self.get_specify_namespace_api(&namespace);
        let Some(secret) = secret_api.get_opt(secret_name).await? else {
            tracing::warn!(
                wasm_plugin = %plugin.name_any(),
                namespace = %namespace,
                secret = %secret_name,
                "WasmPlugin imagePullSecret not found"
            );
            return Ok(None);
        };
        Ok(plugin.oci_registry().and_then(|registry| oci_auth_from_secret(&secret, &registry)))
    }
}

pub(crate) fn oci_auth_from_secret(secret: &Secret, registry: &str) -> Option<Value> {
    let data = secret.data.as_ref()?;
    if let Some(bytes) = data.get(".dockerconfigjson").or_else(|| data.get(".dockercfg")) {
        if let Some(auth) = oci_auth_from_docker_config(&bytes.0, registry) {
            return Some(auth);
        }
    }

    let username = secret_data_string(secret, "username").or_else(|| secret_data_string(secret, "user"))?;
    let password = secret_data_string(secret, "password").unwrap_or_default();
    Some(json!({
        "registry": registry,
        "username": username,
        "password": password,
    }))
}

fn oci_auth_from_docker_config(bytes: &[u8], registry: &str) -> Option<Value> {
    let config: Value = serde_json::from_slice(bytes).ok()?;
    let auths = config.get("auths").and_then(Value::as_object)?;
    let entry = auths.get(registry).or_else(|| auths.get(&format!("https://{registry}"))).or_else(|| auths.get(&format!("http://{registry}"))).or_else(|| {
        (registry == "docker.io").then(|| auths.get("https://index.docker.io/v1/").or_else(|| auths.get("index.docker.io")).or_else(|| auths.get("registry-1.docker.io")))?
    })?;

    let identity_token = entry.get("identitytoken").or_else(|| entry.get("identity_token")).and_then(Value::as_str).map(str::to_string);
    if let Some(identity_token) = identity_token.filter(|v| !v.trim().is_empty()) {
        return Some(json!({
            "registry": registry,
            "identity_token": identity_token,
        }));
    }

    let (username, password) = if let (Some(username), Some(password)) = (
        entry.get("username").and_then(Value::as_str).filter(|v| !v.trim().is_empty()),
        entry.get("password").and_then(Value::as_str),
    ) {
        (username.to_string(), password.to_string())
    } else {
        let auth = entry.get("auth").and_then(Value::as_str)?;
        let decoded = general_purpose::STANDARD.decode(auth).ok()?;
        let decoded = String::from_utf8(decoded).ok()?;
        let (username, password) = decoded.split_once(':')?;
        (username.to_string(), password.to_string())
    };

    Some(json!({
        "registry": registry,
        "username": username,
        "password": password,
    }))
}

fn secret_data_string(secret: &Secret, key: &str) -> Option<String> {
    secret.data.as_ref().and_then(|data| data.get(key)).and_then(|bytes| String::from_utf8(bytes.0.clone()).ok())
}
