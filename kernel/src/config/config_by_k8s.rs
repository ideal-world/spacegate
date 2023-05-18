use std::{collections::HashMap, sync::Arc};

use itertools::Itertools;
use k8s_gateway_api::{Gateway, HttpRoute, HttpRouteFilter};
use k8s_openapi::api::core::v1::Secret;
use kube::{
    api::ListParams,
    runtime::{watcher, WatchStreamExt},
    Api, Client, ResourceExt,
};
use tardis::{
    basic::{error::TardisError, result::TardisResult},
    futures_util::{future::join_all, pin_mut, TryStreamExt},
    log,
    tokio::sync::RwLock,
    TardisFuns,
};

use crate::{do_startup, functions::http_route, shutdown};

use super::{
    gateway_dto::{SgGateway, SgListener, SgParameters, SgProtocol, SgTlsConfig, SgTlsMode},
    http_route_dto::{
        SgBackendRef, SgHttpHeaderMatch, SgHttpHeaderMatchType, SgHttpPathMatch, SgHttpPathMatchType, SgHttpQueryMatch, SgHttpQueryMatchType, SgHttpRoute, SgHttpRouteMatch,
        SgHttpRouteRule,
    },
    k8s_crd::SgFilter,
    plugin_filter_dto::SgRouteFilter,
};
use lazy_static::lazy_static;

lazy_static! {
    static ref GATEWAY_NAMES: Arc<RwLock<Vec<String>>> = Arc::new(RwLock::new(Vec::new()));
}

const GATEWAY_CLASS_NAME: &str = "spacegate";

pub async fn init(namespaces: Option<String>) -> TardisResult<Vec<(SgGateway, Vec<SgHttpRoute>)>> {
    let (gateway_api, http_route_api): (Api<Gateway>, Api<HttpRoute>) = if let Some(namespaces) = namespaces {
        (Api::namespaced(get_client().await?, &namespaces), Api::namespaced(get_client().await?, &namespaces))
    } else {
        (Api::all(get_client().await?), Api::all(get_client().await?))
    };

    let gateway_objs = gateway_api
        .list(&ListParams::default())
        .await
        .map_err(|error| TardisError::wrap(&format!("[SG.Config] Kubernetes error: {error:?}"), ""))?
        .into_iter()
        .filter(|gateway_obj| gateway_obj.spec.gateway_class_name == GATEWAY_CLASS_NAME)
        .collect::<Vec<Gateway>>();
    let gateway_objs_generation = gateway_objs
        .iter()
        .map(|gateway_obj| (gateway_obj.metadata.uid.clone().unwrap_or("".to_string()), gateway_obj.metadata.generation.unwrap_or(0)))
        .collect::<HashMap<String, i64>>();
    let gateway_configs = process_gateway_config(gateway_objs.into_iter().collect()).await?;
    let gateway_names = gateway_configs.iter().map(|gateway_config| gateway_config.name.clone()).collect::<Vec<String>>();

    let http_route_objs: Vec<HttpRoute> = http_route_api
        .list(&ListParams::default())
        .await
        .map_err(|error| TardisError::wrap(&format!("[SG.Config] Kubernetes error: {error:?}"), ""))?
        .into_iter()
        .filter(|http_route_obj| {
            http_route_obj
                .spec
                .inner
                .parent_refs
                .as_ref()
                .map(|parent_refs| {
                    parent_refs.iter().any(|parent_ref| {
                        gateway_names.contains(&format!(
                            "{}.{}",
                            if let Some(namespaces) = parent_ref.namespace.as_ref() {
                                namespaces.to_string()
                            } else {
                                http_route_obj.namespace().as_ref().unwrap_or(&"default".to_string()).to_string()
                            },
                            parent_ref.name
                        ))
                    })
                })
                .unwrap_or(false)
        })
        .collect::<Vec<HttpRoute>>();
    let http_route_objs_generation = http_route_objs
        .iter()
        .map(|http_route_obj| {
            (
                http_route_obj.metadata.uid.clone().unwrap_or("".to_string()),
                http_route_obj.metadata.generation.unwrap_or(0),
            )
        })
        .collect::<HashMap<String, i64>>();
    let http_route_configs: Vec<SgHttpRoute> = process_http_route_config(http_route_objs.into_iter().collect()).await?;

    let config = gateway_configs
        .into_iter()
        .map(|gateway_config| {
            let http_route_configs: Vec<SgHttpRoute> =
                http_route_configs.iter().filter(|http_route_config| http_route_config.gateway_name == gateway_config.name).cloned().collect::<Vec<SgHttpRoute>>();
            (gateway_config, http_route_configs)
        })
        .collect();

    {
        let mut gateway_names_guard = GATEWAY_NAMES.write().await;
        *gateway_names_guard = gateway_names;
    }

    let http_route_api_clone = http_route_api.clone();

    tardis::tokio::spawn(async move {
        let ew = watcher(gateway_api, ListParams::default()).touched_objects();
        pin_mut!(ew);
        while let Some(gateway_obj) = ew.try_next().await.unwrap() {
            if gateway_objs_generation.get(gateway_obj.metadata.uid.as_ref().unwrap_or(&"".to_string())).unwrap_or(&0) == &gateway_obj.metadata.generation.unwrap_or(0) {
                // ignore the original object
                continue;
            }
            if gateway_obj.spec.gateway_class_name != GATEWAY_CLASS_NAME {
                continue;
            }
            log::trace!("[SG.Config] Gateway config change found");

            let gateway_api: Api<Gateway> = Api::namespaced(get_client().await.unwrap(), gateway_obj.namespace().as_ref().unwrap_or(&"default".to_string()));
            match gateway_api.get_metadata_opt(gateway_obj.metadata.name.as_ref().unwrap_or(&"".to_string())).await {
                Ok(has_gateway_obj) => {
                    let gateway_config = process_gateway_config(vec![gateway_obj]).await.unwrap().get(0).unwrap().clone();

                    if has_gateway_obj.is_some() {
                        {
                            let mut gateway_names_guard = GATEWAY_NAMES.write().await;
                            gateway_names_guard.push(gateway_config.name.clone());
                        }
                        let gateway_names_guard = GATEWAY_NAMES.read().await;
                        let http_route_objs = http_route_api_clone
                            .list(&ListParams::default())
                            .await
                            .map_err(|error| TardisError::wrap(&format!("[SG.Config] Kubernetes error: {error:?}"), ""))
                            .unwrap()
                            .into_iter()
                            .filter(|http_route_obj| {
                                http_route_obj
                                    .spec
                                    .inner
                                    .parent_refs
                                    .as_ref()
                                    .map(|parent_refs| {
                                        parent_refs.iter().any(|parent_ref| {
                                            gateway_names_guard.contains(&format!(
                                                "{}.{}",
                                                if let Some(namespaces) = parent_ref.namespace.as_ref() {
                                                    namespaces.to_string()
                                                } else {
                                                    http_route_obj.namespace().as_ref().unwrap_or(&"default".to_string()).to_string()
                                                },
                                                parent_ref.name
                                            ))
                                        })
                                    })
                                    .unwrap_or(false)
                            })
                            .collect::<Vec<HttpRoute>>();
                        let http_route_configs: Vec<SgHttpRoute> = process_http_route_config(http_route_objs.into_iter().collect())
                            .await
                            .map_err(|error| TardisError::wrap(&format!("[SG.Config] Kubernetes error: {error:?}"), ""))
                            .unwrap();
                        shutdown(&gateway_config.name).await.unwrap();
                        log::trace!("[SG.Config] Gateway config change to:{:?}", gateway_config);
                        do_startup(gateway_config, http_route_configs).await.unwrap();
                    } else {
                        {
                            let mut gateway_names_guard = GATEWAY_NAMES.write().await;
                            gateway_names_guard.retain(|name| name != &gateway_config.name);
                        }
                        shutdown(&gateway_config.name).await.unwrap();
                    }
                }
                Err(error) => {
                    log::warn!("[SG.Config] Gateway config change process error:{error}");
                }
            }
        }
    });

    tardis::tokio::spawn(async move {
        let http_route_api_clone = http_route_api.clone();
        let ew = watcher(http_route_api_clone, ListParams::default()).touched_objects();
        pin_mut!(ew);
        while let Some(http_route_obj) = ew.try_next().await.unwrap() {
            if http_route_objs_generation.get(http_route_obj.metadata.uid.as_ref().unwrap_or(&"".to_string())).unwrap_or(&0) == &http_route_obj.metadata.generation.unwrap_or(0) {
                // ignore the original object
                continue;
            }
            if http_route_obj.spec.inner.parent_refs.is_none() {
                continue;
            }
            let (rel_gateway_namespaces, rel_gateway_name) = (
                if let Some(namespaces) = http_route_obj.spec.inner.parent_refs.as_ref().unwrap()[0].namespace.as_ref() {
                    namespaces.to_string()
                } else {
                    http_route_obj.namespace().as_ref().unwrap_or(&"default".to_string()).to_string()
                },
                http_route_obj.spec.inner.parent_refs.as_ref().unwrap()[0].name.clone(),
            );
            let gateway_api: Api<Gateway> = Api::namespaced(get_client().await.unwrap(), &rel_gateway_namespaces);
            let gateway_obj = if let Ok(Some(gateway_obj)) = gateway_api.get_opt(&rel_gateway_name).await {
                if gateway_obj.spec.gateway_class_name != GATEWAY_CLASS_NAME {
                    continue;
                }
                gateway_obj
            } else {
                continue;
            };

            log::trace!("[SG.Config] Http route config change found");

            let gateway_config = process_gateway_config(vec![gateway_obj]).await.unwrap().get(0).unwrap().clone();

            let gateway_names_guard = GATEWAY_NAMES.read().await;

            let http_route_objs: Vec<HttpRoute> = http_route_api
                .list(&ListParams::default())
                .await
                .map_err(|error| TardisError::wrap(&format!("[SG.Config] Kubernetes error: {error:?}"), ""))
                .unwrap()
                .into_iter()
                .filter(|http_route_obj| {
                    http_route_obj
                        .spec
                        .inner
                        .parent_refs
                        .as_ref()
                        .map(|parent_refs| {
                            parent_refs.iter().any(|parent_ref| {
                                gateway_names_guard.contains(&format!(
                                    "{}.{}",
                                    if let Some(namespaces) = parent_ref.namespace.as_ref() {
                                        namespaces.to_string()
                                    } else {
                                        http_route_obj.namespace().as_ref().unwrap_or(&"default".to_string()).to_string()
                                    },
                                    parent_ref.name
                                ))
                            })
                        })
                        .unwrap_or(false)
                })
                .collect::<Vec<HttpRoute>>();

            if http_route_objs.is_empty() {
                http_route::init(gateway_config, vec![]).await.unwrap();
            } else {
                let http_route_configs: Vec<SgHttpRoute> = process_http_route_config(http_route_objs).await.unwrap();
                http_route::init(gateway_config, http_route_configs).await.unwrap();
            }
        }
    });

    Ok(config)
}

async fn process_gateway_config(gateway_objs: Vec<Gateway>) -> TardisResult<Vec<SgGateway>> {
    let mut gateway_configs = Vec::new();

    for gateway_obj in gateway_objs {
        // Key configuration compatibility checks
        if gateway_obj.spec.listeners.iter().any(|listener| listener.hostname.is_some()) {
            return Err(TardisError::not_implemented("[SG.Config] Gateway [spec.listener.hostname] not supported yet", ""));
        }
        if gateway_obj.spec.addresses.is_some() {
            return Err(TardisError::not_implemented("[SG.Config] Gateway [spec.addresses] not supported yet", ""));
        }
        if gateway_obj.spec.listeners.iter().any(|listener| listener.protocol.to_lowercase() != "https" && listener.protocol.to_lowercase() != "http") {
            return Err(TardisError::not_implemented(
                "[SG.Config] Gateway [spec.listener.protocol!=HTTPS|HTTP] not supported yet",
                "",
            ));
        }
        if gateway_obj
            .spec
            .listeners
            .iter()
            .any(|listener| listener.tls.as_ref().map(|tls| tls.mode.as_ref().map(|mode| mode.to_lowercase() != "terminate").unwrap_or(false)).unwrap_or(false))
        {
            return Err(TardisError::not_implemented(
                "[SG.Config] Gateway [spec.listener.tls.mode!=TERMINATE] not supported yet",
                "",
            ));
        }
        if gateway_obj.spec.listeners.iter().any(|listener| listener.tls.as_ref().map(|tls| tls.options.is_some()).unwrap_or(false)) {
            return Err(TardisError::not_implemented("[SG.Config] Gateway [spec.listener.tls.options] not supported yet", ""));
        }

        // Key configuration legality checks
        if gateway_obj.metadata.name.is_none() {
            return Err(TardisError::format_error("[SG.Config] Gateway [metadata.name] is required", ""));
        }
        if gateway_obj.spec.listeners.iter().any(|listener| (listener.protocol.to_lowercase() == "https" || listener.protocol.to_lowercase() == "tls") && listener.tls.is_none()) {
            return Err(TardisError::format_error(
                "[SG.Config] Gateway [spec.listener.tls] is required when the Protocol field is “HTTPS” or “TLS”",
                "",
            ));
        }
        if gateway_obj.spec.listeners.iter().any(|listener| {
            listener.tls.is_some() && (listener.tls.as_ref().unwrap().certificate_refs.is_none() || listener.tls.as_ref().unwrap().certificate_refs.as_ref().unwrap().is_empty())
        }) {
            return Err(TardisError::format_error(
                "[SG.Config] Gateway [spec.listener.tls.certificateRefs] is required when the tls field is enabled",
                "",
            ));
        }
        // Generate gateway configuration
        let gateway_name_without_namespace = gateway_obj.metadata.name.as_ref().unwrap();
        let gateway_config = SgGateway {
            name: format!("{}.{}", gateway_obj.namespace().unwrap_or("default".to_string()), gateway_name_without_namespace),
            parameters: SgParameters {
                redis_url: gateway_obj.metadata.annotations.clone().and_then(|ann| ann.get("redis_url").map(|v| v.to_string())),
                log_level: gateway_obj.metadata.annotations.and_then(|ann: std::collections::BTreeMap<String, String>| ann.get("log_level").map(|v| v.to_string())),
            },
            listeners: join_all(
                gateway_obj
                    .spec
                    .listeners
                    .into_iter()
                    .map(|listener| async move {
                        let tls = match listener.tls {
                            Some(tls) => {
                                let certificate_ref = tls.certificate_refs.as_ref().unwrap().get(0).unwrap();
                                let secret_api: Api<Secret> = if let Some(namespace) = &certificate_ref.namespace {
                                    Api::namespaced(get_client().await.unwrap(), namespace)
                                } else {
                                    Api::all(get_client().await.unwrap())
                                };
                                let secret_obj = secret_api
                                    .get(&certificate_ref.name)
                                    .await
                                    .map_err(|error| TardisError::wrap(&format!("[SG.Config] Kubernetes error: {error:?}"), ""))
                                    .unwrap();
                                let secret_data = secret_obj
                                    .data
                                    .ok_or_else(|| TardisError::format_error(&format!("[SG.Config] Gateway tls secret [{}] data is required", certificate_ref.name), ""))
                                    .unwrap();
                                let tls_crt = secret_data
                                    .get("tls.crt")
                                    .ok_or_else(|| TardisError::format_error(&format!("[SG.Config] Gateway tls secret [{}] data [tls.crt] is required", certificate_ref.name), ""))
                                    .unwrap();
                                let tls_key = secret_data
                                    .get("tls.key")
                                    .ok_or_else(|| TardisError::format_error(&format!("[SG.Config] Gateway tls secret [{}] data [tls.key] is required", certificate_ref.name), ""))
                                    .unwrap();
                                Some(SgTlsConfig {
                                    mode: SgTlsMode::from(tls.mode).unwrap_or_default(),
                                    key: serde_json::to_string(tls_key).unwrap(),
                                    cert: serde_json::to_string(tls_crt).unwrap(),
                                })
                            }
                            None => None,
                        };
                        let sg_listener = SgListener {
                            name: Some(listener.name),
                            ip: None,
                            port: listener.port,
                            protocol: match listener.protocol.to_lowercase().as_str() {
                                "http" => SgProtocol::Http,
                                "https" => SgProtocol::Https,
                                _ => {
                                    return Err(TardisError::not_implemented(
                                        &format!("[SG.Config] Gateway [spec.listener.protocol={}] not supported yet", listener.protocol),
                                        "",
                                    ))
                                }
                            },
                            tls,
                        };
                        Ok(sg_listener)
                    })
                    .collect_vec(),
            )
            .await
            .into_iter()
            .map(|listener| listener.unwrap())
            .collect(),
            filters: get_filters_from_cdr("gateway", gateway_name_without_namespace, &gateway_obj.metadata.namespace).await?,
        };
        gateway_configs.push(gateway_config);
    }
    Ok(gateway_configs)
}

async fn process_http_route_config(http_route_objs: Vec<HttpRoute>) -> TardisResult<Vec<SgHttpRoute>> {
    let mut http_route_configs = Vec::new();

    for http_route_obj in http_route_objs {
        // Key configuration compatibility checks
        if http_route_obj.spec.inner.parent_refs.as_ref().map(|refs| refs.len() > 1).unwrap_or(false) {
            return Err(TardisError::not_implemented(
                "[SG.Config] HttpRoute [spec.parentRefs] multiple values are not supported yet",
                "",
            ));
        }
        if http_route_obj
            .spec
            .rules
            .as_ref()
            .map(|rules| {
                rules.iter().any(|rule| {
                    rule.backend_refs
                        .as_ref()
                        .map(|backends| {
                            backends.iter().any(|backend| {
                                backend.backend_ref.is_some()
                                    && backend.backend_ref.as_ref().unwrap().inner.kind.as_ref().map(|kind| kind.to_lowercase() != "service").unwrap_or(false)
                            })
                        })
                        .unwrap_or(false)
                })
            })
            .unwrap_or(false)
        {
            return Err(TardisError::not_implemented(
                "[SG.Config] HttpRoute [spec.rules.backendRefs.kind!=Service] not supported yet",
                "",
            ));
        }
        // Key configuration legality checks
        if http_route_obj.spec.inner.parent_refs.is_none() {
            return Err(TardisError::format_error("[SG.Config] HttpRoute [spec.parentRefs] is required", ""));
        }
        // Generate gateway configuration
        let rel_gateway_name = format!(
            "{}.{}",
            if let Some(namespaces) = http_route_obj.spec.inner.parent_refs.as_ref().unwrap()[0].namespace.as_ref() {
                namespaces.to_string()
            } else {
                http_route_obj.namespace().as_ref().unwrap_or(&"default".to_string()).to_string()
            },
            http_route_obj.spec.inner.parent_refs.as_ref().unwrap()[0].name
        );
        let http_route_config = SgHttpRoute {
            gateway_name: rel_gateway_name,
            hostnames: http_route_obj.spec.hostnames,
            filters: if let Some(name) = http_route_obj.metadata.name {
                get_filters_from_cdr("httproute", &name, &http_route_obj.metadata.namespace).await?
            } else {
                None
            },
            rules: match http_route_obj.spec.rules {
                Some(rules) => {
                    let sg_rules = rules
                        .into_iter()
                        .map(|rule| SgHttpRouteRule {
                            matches: rule.matches.map(|matches| {
                                matches
                                    .into_iter()
                                    .map(|a_match| SgHttpRouteMatch {
                                        path: a_match.path.map(|path| match path {
                                            k8s_gateway_api::HttpPathMatch::Exact { value } => SgHttpPathMatch {
                                                kind: SgHttpPathMatchType::Exact,
                                                value,
                                            },
                                            k8s_gateway_api::HttpPathMatch::PathPrefix { value } => SgHttpPathMatch {
                                                kind: SgHttpPathMatchType::Prefix,
                                                value,
                                            },
                                            k8s_gateway_api::HttpPathMatch::RegularExpression { value } => SgHttpPathMatch {
                                                kind: SgHttpPathMatchType::Regular,
                                                value,
                                            },
                                        }),
                                        header: a_match.headers.map(|headers| {
                                            headers
                                                .into_iter()
                                                .map(|header| match header {
                                                    k8s_gateway_api::HttpHeaderMatch::Exact { name, value } => SgHttpHeaderMatch {
                                                        kind: SgHttpHeaderMatchType::Exact,
                                                        name,
                                                        value,
                                                    },
                                                    k8s_gateway_api::HttpHeaderMatch::RegularExpression { name, value } => SgHttpHeaderMatch {
                                                        kind: SgHttpHeaderMatchType::Regular,
                                                        name,
                                                        value,
                                                    },
                                                })
                                                .collect_vec()
                                        }),
                                        query: a_match.query_params.map(|query_params| {
                                            query_params
                                                .into_iter()
                                                .map(|query_param| match query_param {
                                                    k8s_gateway_api::HttpQueryParamMatch::Exact { name, value } => SgHttpQueryMatch {
                                                        kind: SgHttpQueryMatchType::Exact,
                                                        name,
                                                        value,
                                                    },
                                                    k8s_gateway_api::HttpQueryParamMatch::RegularExpression { name, value } => SgHttpQueryMatch {
                                                        kind: SgHttpQueryMatchType::Regular,
                                                        name,
                                                        value,
                                                    },
                                                })
                                                .collect_vec()
                                        }),
                                        method: a_match.method.map(|method| vec![method.to_lowercase()]),
                                    })
                                    .collect_vec()
                            }),
                            filters: convert_filters(rule.filters),
                            backends: rule.backend_refs.map(|backends| {
                                backends
                                    .into_iter()
                                    .map(|backend| {
                                        let filters = convert_filters(backend.filters);
                                        let backend = backend.backend_ref.unwrap();
                                        SgBackendRef {
                                            name_or_host: backend.inner.name,
                                            namespace: Some(backend.inner.namespace.unwrap_or("default".to_string())),
                                            port: backend.inner.port.unwrap(),
                                            timeout_ms: None,
                                            protocol: None,
                                            weight: backend.weight,
                                            filters,
                                        }
                                    })
                                    .collect_vec()
                            }),
                            timeout_ms: None,
                        })
                        .collect_vec();
                    Some(sg_rules)
                }
                None => None,
            },
        };
        http_route_configs.push(http_route_config);
    }
    Ok(http_route_configs)
}

async fn get_filters_from_cdr(kind: &str, name: &str, namespace: &Option<String>) -> TardisResult<Option<Vec<SgRouteFilter>>> {
    let filter_api: Api<SgFilter> = Api::all(get_client().await?);

    let filter_objs: Vec<SgRouteFilter> = filter_api
        .list(&ListParams::default())
        .await
        .map_err(|error| TardisError::wrap(&format!("[SG.Config] Kubernetes error: {error:?}"), ""))?
        .into_iter()
        .filter(|filter_obj| {
            filter_obj.spec.target_refs.iter().any(|target_ref| {
                target_ref.kind.to_lowercase() == kind.to_lowercase()
                    && target_ref.name.to_lowercase() == name.to_lowercase()
                    && target_ref.namespace.as_ref().unwrap_or(&"default".to_string()).to_lowercase() == namespace.as_ref().unwrap_or(&"default".to_string()).to_lowercase()
            })
        })
        .flat_map(|filter_obj| {
            filter_obj.spec.filters.into_iter().map(|filter| SgRouteFilter {
                code: filter.code,
                name: filter.name,
                spec: filter.config,
            })
        })
        .collect();
    if filter_objs.is_empty() {
        Ok(None)
    } else {
        Ok(Some(filter_objs))
    }
}

fn convert_filters(filters: Option<Vec<HttpRouteFilter>>) -> Option<Vec<SgRouteFilter>> {
    filters
        .map(|filters| {
            filters
                .into_iter()
                .map(|filter| {
                    let sg_filter = match filter {
                        k8s_gateway_api::HttpRouteFilter::RequestHeaderModifier { request_header_modifier } => {
                            let mut sg_sets = HashMap::new();
                            if let Some(adds) = request_header_modifier.add {
                                for add in adds {
                                    sg_sets.insert(add.name, add.value);
                                }
                            }
                            if let Some(sets) = request_header_modifier.set {
                                for set in sets {
                                    sg_sets.insert(set.name, set.value);
                                }
                            }
                            SgRouteFilter {
                                code: crate::plugins::filters::header_modifier::CODE.to_string(),
                                name: None,
                                spec: TardisFuns::json
                                    .obj_to_json(&crate::plugins::filters::header_modifier::SgFilterHeaderModifier {
                                        kind: crate::plugins::filters::header_modifier::SgFilterHeaderModifierKind::Request,
                                        sets: if sg_sets.is_empty() { None } else { Some(sg_sets) },
                                        remove: request_header_modifier.remove,
                                    })
                                    .unwrap(),
                            }
                        }
                        k8s_gateway_api::HttpRouteFilter::RequestRedirect { request_redirect } => SgRouteFilter {
                            code: crate::plugins::filters::redirect::CODE.to_string(),
                            name: None,
                            spec: TardisFuns::json
                                .obj_to_json(&crate::plugins::filters::redirect::SgFilterRedirect {
                                    scheme: request_redirect.scheme,
                                    hostname: request_redirect.hostname,
                                    path: request_redirect.path.map(|path| match path {
                                        k8s_gateway_api::HttpPathModifier::ReplaceFullPath { replace_full_path } => super::plugin_filter_dto::SgHttpPathModifier {
                                            kind: super::plugin_filter_dto::SgHttpPathModifierType::ReplaceFullPath,
                                            value: replace_full_path,
                                        },
                                        k8s_gateway_api::HttpPathModifier::ReplacePrefixMatch { replace_prefix_match } => super::plugin_filter_dto::SgHttpPathModifier {
                                            kind: super::plugin_filter_dto::SgHttpPathModifierType::ReplacePrefixMatch,
                                            value: replace_prefix_match,
                                        },
                                    }),
                                    port: request_redirect.port,
                                    status_code: request_redirect.status_code,
                                })
                                .unwrap(),
                        },
                        k8s_gateway_api::HttpRouteFilter::URLRewrite { url_rewrite } => SgRouteFilter {
                            code: crate::plugins::filters::rewrite::CODE.to_string(),
                            name: None,
                            spec: TardisFuns::json
                                .obj_to_json(&crate::plugins::filters::rewrite::SgFilterRewrite {
                                    hostname: url_rewrite.hostname,
                                    path: url_rewrite.path.map(|path| match path {
                                        k8s_gateway_api::HttpPathModifier::ReplaceFullPath { replace_full_path } => super::plugin_filter_dto::SgHttpPathModifier {
                                            kind: super::plugin_filter_dto::SgHttpPathModifierType::ReplaceFullPath,
                                            value: replace_full_path,
                                        },
                                        k8s_gateway_api::HttpPathModifier::ReplacePrefixMatch { replace_prefix_match } => super::plugin_filter_dto::SgHttpPathModifier {
                                            kind: super::plugin_filter_dto::SgHttpPathModifierType::ReplacePrefixMatch,
                                            value: replace_prefix_match,
                                        },
                                    }),
                                })
                                .unwrap(),
                        },
                        k8s_gateway_api::HttpRouteFilter::RequestMirror { .. } => {
                            return Err(TardisError::not_implemented(
                                "[SG.Config] HttpRoute [spec.rules.filters.type=RequestMirror] not supported yet",
                                "",
                            ))
                        }
                        k8s_gateway_api::HttpRouteFilter::ExtensionRef { .. } => {
                            return Err(TardisError::not_implemented(
                                "[SG.Config] HttpRoute [spec.rules.filters.type=ExtensionRef] not supported yet",
                                "",
                            ))
                        }
                    };
                    Ok(sg_filter)
                })
                .collect_vec()
        })
        .map(|filters| filters.into_iter().map(|filter| filter.unwrap()).collect_vec())
}

async fn get_client() -> TardisResult<Client> {
    Client::try_default().await.map_err(|error| TardisError::wrap(&format!("[SG.Config] Kubernetes error: {error:?}"), ""))
}
