use std::collections::HashMap;

use futures_util::future::join_all;
use gateway::{SgListener, SgParameters, SgProtocolConfig, SgTlsConfig, SgTlsMode};
use http_route::{BackendHost, SgBackendRef, SgHttpHeaderMatch, SgHttpPathMatch, SgHttpQueryMatch, SgHttpRouteMatch, SgHttpRouteRule};
use k8s_gateway_api::{Gateway, HttpHeaderMatch, HttpPathMatch, HttpQueryParamMatch, HttpRequestHeaderFilter, HttpRoute, HttpRouteFilter, HttpRouteMatch, HttpRouteRule, Listener};
use k8s_openapi::api::core::v1::Secret;
use kube::{Api, ResourceExt};

use super::Retrieve;
use crate::{
    constants::{self, GATEWAY_CLASS_NAME},
    k8s_crd::{
        http_spaceroute::{self, HttpBackendRef, HttpSpaceroute},
        sg_filter::{K8sSgFilterSpecTargetRef, SgFilter, SgFilterTargetKind},
    },
    model::{
        gateway,
        gatewayapi_support_filter::{
            SgFilterHeaderModifier, SgFilterHeaderModifierKind, SgFilterRedirect, SgFilterRewrite, SgHttpPathModifier, SgHttpPathModifierType, SG_FILTER_HEADER_MODIFIER_CODE,
            SG_FILTER_REDIRECT_CODE, SG_FILTER_REWRITE_CODE,
        },
        http_route, SgGateway, SgHttpRoute, SgRouteFilter,
    },
    service::backend::k8s::K8s,
    BoxError, BoxResult,
};

impl Retrieve for K8s {
    async fn retrieve_config_item_gateway(&self, gateway_name: &str) -> Result<Option<SgGateway>, BoxError> {
        let gateway_api: Api<Gateway> = self.get_namespace_api();

        let result = if let Some(gateway_obj) = gateway_api.get_opt(&gateway_name).await?.and_then(|gateway_obj| {
            if gateway_obj.spec.gateway_class_name == GATEWAY_CLASS_NAME {
                Some(gateway_obj)
            } else {
                None
            }
        }) {
            Some(self.from_kube_gateway(gateway_obj).await?)
        } else {
            None
        };

        Ok(result)
    }

    async fn retrieve_config_item_route(&self, gateway_name: &str, route_name: &str) -> Result<Option<SgHttpRoute>, BoxError> {
        let http_spaceroute_api: Api<HttpSpaceroute> = self.get_namespace_api();
        let httproute_api: Api<HttpRoute> = self.get_namespace_api();

        let result = if let Some(httpspaceroute) = http_spaceroute_api.get_opt(&route_name).await?.and_then(|http_route_obj| {
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
            Some(self.from_kube_httpspaceroute(httpspaceroute).await?)
        } else {
            if let Some(http_route) = httproute_api.get_opt(&route_name).await?.and_then(|http_route| {
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
                Some(self.from_kube_httproute(http_route).await?)
            } else {
                None
            }
        };

        Ok(result)
    }

    async fn retrieve_config_item_route_names(&self, name: &str) -> Result<Vec<String>, BoxError> {
        todo!()
    }

    async fn retrieve_config_names(&self) -> Result<Vec<String>, BoxError> {
        todo!()
    }
}

impl K8s {
    async fn from_kube_gateway(&self, gateway_obj: Gateway) -> Result<SgGateway, BoxError> {
        let gateway_name = gateway_obj.name_any();
        let filters = self
            .retrieve_config_item_filters(K8sSgFilterSpecTargetRef {
                kind: SgFilterTargetKind::Gateway.into(),
                name: gateway_name.clone(),
                namespace: gateway_obj.namespace(),
            })
            .await?;
        let result = SgGateway {
            name: gateway_name,
            parameters: SgParameters::from_kube_gateway(&gateway_obj),
            listeners: self.retrieve_config_item_listeners(&gateway_obj.spec.listeners).await?,
            filters,
        };
        Ok(result)
    }

    async fn from_kube_httpspaceroute(&self, httpspace_route: HttpSpaceroute) -> Result<SgHttpRoute, BoxError> {
        let kind = if let Some(kind) = httpspace_route.annotations().get(constants::RAW_HTTP_ROUTE_KIND) {
            kind.clone()
        } else {
            SgFilterTargetKind::Httpspaceroute.into()
        };
        let priority = httpspace_route.annotations().get(crate::constants::ANNOTATION_RESOURCE_PRIORITY).and_then(|a| a.parse::<u16>().ok()).unwrap_or(0);
        let gateway_refs = httpspace_route.spec.inner.parent_refs.clone().unwrap_or_default();
        let filters = self
            .retrieve_config_item_filters(K8sSgFilterSpecTargetRef {
                kind,
                name: httpspace_route.name_any(),
                namespace: httpspace_route.namespace(),
            })
            .await?;
        Ok(SgHttpRoute {
            gateway_name: gateway_refs.get(0).map(|x| x.name.clone()).unwrap_or_default(),
            hostnames: httpspace_route.spec.hostnames.clone(),
            filters,
            rules: httpspace_route.spec.rules.map(|r_vec| r_vec.into_iter().map(SgHttpRouteRule::from_kube_httproute).collect::<Result<Vec<_>, BoxError>>()).transpose()?.unwrap_or_default(),
            priority,
        })
    }

    async fn from_kube_httproute(&self, http_route: HttpRoute) -> Result<SgHttpRoute, BoxError> {
        self.from_kube_httpspaceroute(http_route.into()).await
    }

    async fn retrieve_config_item_filters(&self, target: K8sSgFilterSpecTargetRef) -> Result<Vec<SgRouteFilter>, BoxError> {
        todo!()
    }

    async fn retrieve_config_item_listeners(&self, listeners: &Vec<Listener>) -> Result<Vec<SgListener>, BoxError> {
        join_all(
            listeners
                .into_iter()
                .map(|listener| async move {
                    let sg_listener = SgListener {
                        name: Some(listener.name.clone()),
                        ip: None,
                        port: listener.port,
                        protocol: match listener.protocol.to_lowercase().as_str() {
                            "http" => SgProtocolConfig::Http,
                            "https" => {
                                if let Some(tls_config) = &listener.tls {
                                    if let Some(certificate_ref) = tls_config.certificate_refs.as_ref().and_then(|vec| vec.get(0)) {
                                        let secret_api: Api<Secret> = self.get_namespace_api();
                                        if let Some(secret_obj) = secret_api.get_opt(&certificate_ref.name).await? {
                                            let tls = if let Some(secret_data) = secret_obj.data {
                                                if let Some(tls_crt) = secret_data.get("tls.crt") {
                                                    if let Some(tls_key) = secret_data.get("tls.key") {
                                                        Some(SgTlsConfig {
                                                            mode: tls_config.mode.clone().into(),
                                                            key: String::from_utf8(tls_key.0.clone()).expect("[SG.Config] Gateway tls secret [tls.key] is not valid utf8"),
                                                            cert: String::from_utf8(tls_crt.0.clone()).expect("[SG.Config] Gateway tls secret [tls.cert] is not valid utf8"),
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
}

impl SgParameters {
    fn from_kube_gateway(gateway: &Gateway) -> Self {
        let gateway_annotations = gateway.metadata.annotations.clone();
        if let Some(gateway_annotations) = gateway_annotations {
            SgParameters {
                redis_url: gateway_annotations.get(crate::constants::GATEWAY_ANNOTATION_REDIS_URL).map(|v| v.to_string()),
                log_level: gateway_annotations.get(crate::constants::GATEWAY_ANNOTATION_LOG_LEVEL).map(|v| v.to_string()),
                lang: gateway_annotations.get(crate::constants::GATEWAY_ANNOTATION_LANGUAGE).map(|v| v.to_string()),
                ignore_tls_verification: gateway_annotations.get(crate::constants::GATEWAY_ANNOTATION_IGNORE_TLS_VERIFICATION).and_then(|v| v.parse::<bool>().ok()),
            }
        } else {
            SgParameters {
                redis_url: None,
                log_level: None,
                lang: None,
                ignore_tls_verification: None,
            }
        }
    }
}

impl SgHttpRouteRule {
    fn from_kube_httproute(rule: http_spaceroute::HttpRouteRule) -> Result<SgHttpRouteRule, BoxError> {
        Ok(SgHttpRouteRule {
            matches: rule.matches.map(|m_vec| m_vec.into_iter().map(SgHttpRouteMatch::from_kube_httproute).collect::<Vec<_>>()),
            filters: rule.filters.map(|f_vec| f_vec.into_iter().map(SgRouteFilter::from_http_route_filter).collect::<Result<Vec<_>, BoxError>>()).transpose()?.unwrap_or_default(),
            backends: rule
                .backend_refs
                .map(|b_vec| b_vec.into_iter().filter_map(|b| SgBackendRef::from_kube_httproute(b).transpose()).collect::<Result<Vec<_>, BoxError>>())
                .transpose()?
                .unwrap_or_default(),
            timeout_ms: rule.timeout_ms,
        })
    }
}

impl SgHttpRouteMatch {
    pub(crate) fn from_kube_httproute(route_match: HttpRouteMatch) -> SgHttpRouteMatch {
        SgHttpRouteMatch {
            method: route_match.method.map(|m_vec| vec![http_route::SgHttpMethodMatch(m_vec)]),
            path: route_match.path.map(SgHttpPathMatch::from_kube_httproute),
            header: route_match.headers.map(|h_vec| h_vec.into_iter().map(SgHttpHeaderMatch::from_kube_httproute).collect::<Vec<_>>()),
            query: route_match.query_params.map(|q_vec| q_vec.into_iter().map(SgHttpQueryMatch::from_kube_httproute).collect::<Vec<_>>()),
        }
    }
}

impl SgHttpPathMatch {
    pub(crate) fn from_kube_httproute(path_match: HttpPathMatch) -> SgHttpPathMatch {
        match path_match {
            HttpPathMatch::Exact { value } => SgHttpPathMatch::Exact(value),
            HttpPathMatch::PathPrefix { value } => SgHttpPathMatch::Prefix(value),
            HttpPathMatch::RegularExpression { value } => SgHttpPathMatch::Regular(value),
        }
    }
}

impl SgHttpHeaderMatch {
    pub(crate) fn from_kube_httproute(header_match: HttpHeaderMatch) -> SgHttpHeaderMatch {
        match header_match {
            HttpHeaderMatch::Exact { name, value } => SgHttpHeaderMatch::Exact { name, value },
            HttpHeaderMatch::RegularExpression { name, value } => SgHttpHeaderMatch::Regular { name, re: value },
        }
    }
}

impl SgHttpQueryMatch {
    pub(crate) fn from_kube_httproute(query_match: HttpQueryParamMatch) -> SgHttpQueryMatch {
        match query_match {
            HttpQueryParamMatch::Exact { name, value } => SgHttpQueryMatch::Exact { key: name, value: value },
            HttpQueryParamMatch::RegularExpression { name, value } => SgHttpQueryMatch::Regular { key: name, re: value },
        }
    }
}

impl SgBackendRef {
    pub(crate) fn from_kube_httproute(http_backend: HttpBackendRef) -> Result<Option<SgBackendRef>, BoxError> {
        http_backend
            .backend_ref
            .map(|backend| {
                let protocol = if let Some(kind) = backend.inner.kind.as_ref() {
                    match kind.as_str() {
                        BANCKEND_KIND_EXTERNAL_HTTP => Some(gateway::SgBackendProtocol::Http),
                        BANCKEND_KIND_EXTERNAL_HTTPS => Some(gateway::SgBackendProtocol::Https),
                        _ => None,
                    }
                } else {
                    None
                };
                Ok(SgBackendRef {
                    host: BackendHost::default(),
                    port: backend.inner.port.unwrap_or(80),
                    timeout_ms: backend.timeout_ms,
                    protocol,
                    weight: backend.weight.unwrap_or(1),
                    filters: http_backend
                        .filters
                        .map(|f_vec| f_vec.into_iter().map(SgRouteFilter::from_http_route_filter).collect::<Result<Vec<SgRouteFilter>, BoxError>>())
                        .transpose()?
                        .unwrap_or_default(),
                })
            })
            .transpose()
    }
}

impl SgRouteFilter {
    pub fn from_http_route_filter(route_filter: HttpRouteFilter) -> BoxResult<SgRouteFilter> {
        let process_header_modifier = |header_modifier: HttpRequestHeaderFilter, modifier_kind: SgFilterHeaderModifierKind| -> Result<SgRouteFilter, BoxError> {
            let mut sg_sets = HashMap::new();
            if let Some(adds) = header_modifier.add {
                for add in adds {
                    sg_sets.insert(add.name, add.value);
                }
            }
            if let Some(sets) = header_modifier.set {
                for set in sets {
                    sg_sets.insert(set.name, set.value);
                }
            }

            Ok(SgRouteFilter {
                code: SG_FILTER_HEADER_MODIFIER_CODE.to_string(),
                name: None,
                spec: serde_json::to_value(&SgFilterHeaderModifier {
                    kind: modifier_kind,
                    sets: if sg_sets.is_empty() { None } else { Some(sg_sets) },
                    remove: header_modifier.remove,
                })?,
            })
        };
        let mut sg_filter = match route_filter {
            k8s_gateway_api::HttpRouteFilter::RequestHeaderModifier { request_header_modifier } => {
                process_header_modifier(request_header_modifier, SgFilterHeaderModifierKind::Request)?
            }
            k8s_gateway_api::HttpRouteFilter::ResponseHeaderModifier { response_header_modifier } => {
                process_header_modifier(response_header_modifier, SgFilterHeaderModifierKind::Response)?
            }
            k8s_gateway_api::HttpRouteFilter::RequestRedirect { request_redirect } => SgRouteFilter {
                code: SG_FILTER_REDIRECT_CODE.to_string(),
                name: None,
                spec: serde_json::to_value(&SgFilterRedirect {
                    scheme: request_redirect.scheme,
                    hostname: request_redirect.hostname,
                    path: request_redirect.path.map(|path| match path {
                        k8s_gateway_api::HttpPathModifier::ReplaceFullPath { replace_full_path } => SgHttpPathModifier {
                            kind: SgHttpPathModifierType::ReplaceFullPath,
                            value: replace_full_path,
                        },
                        k8s_gateway_api::HttpPathModifier::ReplacePrefixMatch { replace_prefix_match } => SgHttpPathModifier {
                            kind: SgHttpPathModifierType::ReplacePrefixMatch,
                            value: replace_prefix_match,
                        },
                    }),
                    port: request_redirect.port,
                    status_code: request_redirect.status_code,
                })?,
            },
            k8s_gateway_api::HttpRouteFilter::URLRewrite { url_rewrite } => SgRouteFilter {
                code: SG_FILTER_REWRITE_CODE.to_string(),
                name: None,
                spec: serde_json::to_value(&SgFilterRewrite {
                    hostname: url_rewrite.hostname,
                    path: url_rewrite.path.map(|path| match path {
                        k8s_gateway_api::HttpPathModifier::ReplaceFullPath { replace_full_path } => SgHttpPathModifier {
                            kind: SgHttpPathModifierType::ReplaceFullPath,
                            value: replace_full_path,
                        },
                        k8s_gateway_api::HttpPathModifier::ReplacePrefixMatch { replace_prefix_match } => SgHttpPathModifier {
                            kind: SgHttpPathModifierType::ReplacePrefixMatch,
                            value: replace_prefix_match,
                        },
                    }),
                })?,
            },
            k8s_gateway_api::HttpRouteFilter::RequestMirror { .. } => return Err("[SG.Common] HttpRoute [spec.rules.filters.type=RequestMirror] not supported yet".into()),
            k8s_gateway_api::HttpRouteFilter::ExtensionRef { .. } => return Err("[SG.Common] HttpRoute [spec.rules.filters.type=ExtensionRef] not supported yet".into()),
        };
        Ok(sg_filter)
    }
}
