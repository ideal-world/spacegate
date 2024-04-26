use std::collections::BTreeMap;

use futures_util::future::join_all;
use gateway::SgBackendProtocol;
use http_route::SgHttpRoute;
use hyper::client;
use k8s_gateway_api::{
    BackendObjectReference, CommonRouteSpec, HttpHeaderMatch, HttpPathMatch, HttpPathModifier, HttpQueryParamMatch, HttpRouteFilter, HttpRouteMatch, HttpUrlRewriteFilter,
    ParentReference,
};
use kube::{api::ObjectMeta, ResourceExt};
use spacegate_model::{
    ext::k8s::crd::sg_filter::{K8sSgFilterSpecTargetRef, SgFilterTargetKind},
    PluginInstanceId,
};

use crate::{
    constants,
    ext::k8s::{
        crd::http_spaceroute::{self, BackendRef, HttpBackendRef, HttpRouteRule, HttpSpaceroute, HttpSpacerouteSpec},
        helper_filter::SgSingeFilter,
    },
    gateway, http_route,
    service::k8s::K8s,
    BackendHost, BoxResult, K8sServiceData, PluginConfig, SgBackendRef, SgHttpHeaderMatch, SgHttpPathMatch, SgHttpQueryMatch, SgHttpRouteMatch, SgHttpRouteRule,
};

use super::{filter_k8s_conv::PluginIdConv as _, ToTarget};
pub(crate) trait SgHttpRouteConv {
    /// Convert to HttpSpaceroute and SgSingeFilter
    fn to_kube_httproute(self, gateway_name: &str, name: &str, client: &K8s) -> (HttpSpaceroute, Vec<PluginInstanceId>);
}

impl SgHttpRouteConv for SgHttpRoute {
    fn to_kube_httproute(self, gateway_name: &str, name: &str, client: &K8s) -> (HttpSpaceroute, Vec<PluginInstanceId>) {
        let httproute = HttpSpaceroute {
            metadata: ObjectMeta {
                labels: None,
                name: Some(name.to_string()),
                owner_references: None,
                self_link: None,
                annotations: Some(BTreeMap::from([(constants::ANNOTATION_RESOURCE_PRIORITY.to_string(), self.priority.to_string())])),
                ..Default::default()
            },
            spec: HttpSpacerouteSpec {
                inner: CommonRouteSpec {
                    parent_refs: Some(vec![ParentReference {
                        group: None,
                        kind: Some("Gateway".to_string()),
                        namespace: Some(client.namespace.to_string()),
                        name: gateway_name.to_string(),
                        section_name: None,
                        port: None,
                    }]),
                },
                hostnames: self.hostnames,
                rules: Some(self.rules.into_iter().map(|r| r.into_kube_httproute()).collect::<Vec<_>>()),
            },
            status: None,
        };
        (httproute, self.plugins)
    }
}

pub(crate) trait SgHttpRouteRuleConv {
    /// # to_kube_httproute
    /// `SgHttpRouteRule` to `HttpRouteRule`, include `HttpRouteFilter` and  excluding `SgFilter`.
    fn into_kube_httproute(self) -> HttpRouteRule;
    fn from_kube_httproute(rule: http_spaceroute::HttpRouteRule) -> BoxResult<SgHttpRouteRule>;
}

impl SgHttpRouteRuleConv for SgHttpRouteRule {
    fn into_kube_httproute(self) -> HttpRouteRule {
        let (matches, mut plugins): (Option<Vec<HttpRouteMatch>>, Vec<HttpRouteFilter>) = self
            .matches
            .map(|m_vec| {
                let (matches, plugins): (Vec<_>, Vec<_>) = m_vec.into_iter().map(|m| m.into_kube_httproute()).unzip();
                (Some(matches.into_iter().flatten().collect()), plugins.into_iter().flatten().collect())
            })
            .unwrap_or_default();
        plugins.append(&mut self.plugins.into_iter().filter_map(|p| p.to_http_route_filter()).collect::<Vec<_>>());
        HttpRouteRule {
            matches,
            filters: Some(plugins),
            backend_refs: Some(self.backends.into_iter().map(|b| b.into_kube_httproute()).collect::<Vec<_>>()),
            timeout_ms: self.timeout_ms,
        }
    }

    fn from_kube_httproute(rule: http_spaceroute::HttpRouteRule) -> BoxResult<SgHttpRouteRule> {
        let (ext_plugins, legacy_plugins): (Vec<_>, Vec<_>) =
            rule.filters.map(|f_vec| f_vec.into_iter().partition(|f| matches!(f, HttpRouteFilter::ExtensionRef { extension_ref: _ }))).unwrap_or_default();
        let matches = if let Some(mut matches) = rule.matches {
            if matches.len() > 1 {
                if legacy_plugins.iter().find(|p| matches!(p, HttpRouteFilter::URLRewrite { url_rewrite: _ })).is_some() {
                    return Err("url_rewrite is not supported with multiple matches".into());
                }
                if legacy_plugins.iter().find(|p| matches!(p, HttpRouteFilter::RequestHeaderModifier { request_header_modifier: _ })).is_some() {
                    return Err("request_header_modifier is not supported with multiple matches".into());
                }
                Some(matches.into_iter().map(|m| SgHttpRouteMatch::from_kube_httproute(m)).collect::<Vec<_>>())
            } else if let Some(match_) = matches.pop() {
                let mut m = SgHttpRouteMatch::from_kube_httproute(match_);
                if legacy_plugins.iter().filter(|p| matches!(p, HttpRouteFilter::URLRewrite { url_rewrite: _ })).count() > 1 {
                    return Err("url_rewrite can only have one in each rule".into());
                } else if let Some(url_rewrite) = legacy_plugins.iter().find(|p| matches!(p, HttpRouteFilter::URLRewrite { url_rewrite: _ })) {
                    m.path.map(|m_p| match url_rewrite {
                        HttpRouteFilter::URLRewrite { url_rewrite } => {
                            if let Some(rewrite_path) = url_rewrite.path {
                                match m_p {
                                    SgHttpPathMatch::Exact { value, replace } => match rewrite_path {
                                        HttpPathModifier::ReplaceFullPath { replace_full_path } => SgHttpPathMatch::Exact {
                                            value,
                                            replace: Some(replace_full_path),
                                        },
                                        _ => m_p,
                                    },
                                    SgHttpPathMatch::Prefix { value, replace } => match rewrite_path {
                                        HttpPathModifier::ReplacePrefixMatch { replace_prefix_match } => SgHttpPathMatch::Exact {
                                            value,
                                            replace: Some(replace_prefix_match),
                                        },
                                        _ => m_p,
                                    },
                                    _ => m_p,
                                }
                            } else {
                                m_p
                            }
                        }
                        _ => unreachable!(),
                    });
                }
                Some(vec![m])
            } else {
                Some(vec![])
            }
        } else {
            None
        };
        Ok(SgHttpRouteRule {
            matches,
            plugins: ext_plugins.into_iter().filter_map(PluginInstanceId::from_http_route_filter).collect(),
            backends: rule
                .backend_refs
                .map(|b_vec| b_vec.into_iter().filter_map(|b| SgBackendRef::from_kube_httproute(b).transpose()).collect::<BoxResult<Vec<_>>>())
                .transpose()?
                .unwrap_or_default(),
            timeout_ms: rule.timeout_ms,
        })
    }
}

pub(crate) trait SgHttpRouteMatchConv {
    fn from_kube_httproute(route_match: HttpRouteMatch) -> SgHttpRouteMatch;
    fn into_kube_httproute(self) -> (Vec<HttpRouteMatch>, Vec<HttpRouteFilter>);
}
impl SgHttpRouteMatchConv for SgHttpRouteMatch {
    fn into_kube_httproute(self) -> (Vec<HttpRouteMatch>, Vec<HttpRouteFilter>) {
        let (match_vec, plugins) = if let Some(method_vec) = self.method {
            method_vec
                .into_iter()
                .map(|m| {
                    let (path, plugin) = self
                        .path
                        .clone()
                        .map(|p| {
                            let (path, plugin) = p.into_kube_httproute();
                            (Some(path), plugin)
                        })
                        .unwrap_or((None, None));
                    (
                        HttpRouteMatch {
                            path: path,
                            headers: self.header.clone().map(|h_vec| h_vec.into_iter().map(|h| h.into_kube_httproute()).collect::<Vec<_>>()),
                            query_params: self.query.clone().map(|q_vec| q_vec.into_iter().map(|q| q.into_kube_httproute()).collect::<Vec<_>>()),
                            method: Some(m.0),
                        },
                        plugin,
                    )
                })
                .unzip()
        } else {
            let (path, plugin) = self
                .path
                .clone()
                .map(|p| {
                    let (path, plugin) = p.into_kube_httproute();
                    (Some(path), plugin)
                })
                .unwrap_or((None, None));
            (
                vec![HttpRouteMatch {
                    path: path,
                    headers: self.header.map(|h_vec| h_vec.into_iter().map(|h| h.into_kube_httproute()).collect::<Vec<_>>()),
                    query_params: self.query.map(|q_vec| q_vec.into_iter().map(|q| q.into_kube_httproute()).collect::<Vec<_>>()),
                    method: None,
                }],
                vec![plugin],
            )
        };
        (match_vec, plugins.into_iter().filter_map(|x| x).collect())
    }

    fn from_kube_httproute(route_match: HttpRouteMatch) -> SgHttpRouteMatch {
        SgHttpRouteMatch {
            method: route_match.method.map(|m_vec| vec![http_route::SgHttpMethodMatch(m_vec)]),
            path: route_match.path.map(SgHttpPathMatch::from_kube_httproute),
            header: route_match.headers.map(|h_vec| h_vec.into_iter().map(SgHttpHeaderMatch::from_kube_httproute).collect::<Vec<_>>()),
            query: route_match.query_params.map(|q_vec| q_vec.into_iter().map(SgHttpQueryMatch::from_kube_httproute).collect::<Vec<_>>()),
        }
    }
}

pub(crate) trait SgHttpPathMatchConv {
    fn from_kube_httproute(path_match: HttpPathMatch) -> SgHttpPathMatch;
    fn into_kube_httproute(self) -> (HttpPathMatch, Option<HttpRouteFilter>);
}
impl SgHttpPathMatchConv for SgHttpPathMatch {
    fn into_kube_httproute(self) -> (HttpPathMatch, Option<HttpRouteFilter>) {
        match self {
            SgHttpPathMatch::Exact { value, replace } => (
                HttpPathMatch::Exact { value },
                replace.map(|r| HttpRouteFilter::URLRewrite {
                    url_rewrite: HttpUrlRewriteFilter {
                        hostname: None,
                        path: Some(HttpPathModifier::ReplaceFullPath { replace_full_path: r }),
                    },
                }),
            ),
            SgHttpPathMatch::Prefix { value, replace } => (
                HttpPathMatch::PathPrefix { value },
                replace.map(|r| HttpRouteFilter::URLRewrite {
                    url_rewrite: HttpUrlRewriteFilter {
                        hostname: None,
                        path: Some(HttpPathModifier::ReplacePrefixMatch { replace_prefix_match: r }),
                    },
                }),
            ),
            SgHttpPathMatch::RegExp { value, replace } => (
                HttpPathMatch::RegularExpression { value },
                replace.map(|r| HttpRouteFilter::URLRewrite {
                    url_rewrite: HttpUrlRewriteFilter {
                        hostname: None,
                        path: Some(HttpPathModifier::ReplaceFullPath { replace_full_path: r }),
                    },
                }),
            ),
        }
    }

    fn from_kube_httproute(path_match: HttpPathMatch) -> SgHttpPathMatch {
        match path_match {
            HttpPathMatch::Exact { value } => SgHttpPathMatch::Exact { value, replace: None },
            HttpPathMatch::PathPrefix { value } => SgHttpPathMatch::Prefix { value, replace: None },
            HttpPathMatch::RegularExpression { value } => SgHttpPathMatch::RegExp { value, replace: None },
        }
    }
}

pub(crate) trait SgHttpHeaderMatchConv {
    fn from_kube_httproute(header_match: HttpHeaderMatch) -> SgHttpHeaderMatch;
    fn into_kube_httproute(self) -> HttpHeaderMatch;
}

impl SgHttpHeaderMatchConv for SgHttpHeaderMatch {
    //todo to plugin
    fn into_kube_httproute(self) -> HttpHeaderMatch {
        match self {
            SgHttpHeaderMatch::Exact { name, value, replace } => HttpHeaderMatch::Exact { name, value },
            SgHttpHeaderMatch::RegExp { name, re, replace } => HttpHeaderMatch::RegularExpression { name, value: re },
        }
    }

    fn from_kube_httproute(header_match: HttpHeaderMatch) -> SgHttpHeaderMatch {
        match header_match {
            HttpHeaderMatch::Exact { name, value } => SgHttpHeaderMatch::Exact { name, value, replace: None },
            HttpHeaderMatch::RegularExpression { name, value } => SgHttpHeaderMatch::Regular { name, re: value },
        }
    }
}

pub(crate) trait SgHttpQueryMatchConv {
    fn into_kube_httproute(self) -> HttpQueryParamMatch;
    fn from_kube_httproute(query_match: HttpQueryParamMatch) -> SgHttpQueryMatch;
}

impl SgHttpQueryMatchConv for SgHttpQueryMatch {
    fn into_kube_httproute(self) -> HttpQueryParamMatch {
        match self {
            SgHttpQueryMatch::Exact { key: name, value } => HttpQueryParamMatch::Exact { name, value },
            SgHttpQueryMatch::Regular { key: name, re: value } => HttpQueryParamMatch::RegularExpression { name, value },
        }
    }

    fn from_kube_httproute(query_match: HttpQueryParamMatch) -> SgHttpQueryMatch {
        match query_match {
            HttpQueryParamMatch::Exact { name, value } => SgHttpQueryMatch::Exact { key: name, value },
            HttpQueryParamMatch::RegularExpression { name, value } => SgHttpQueryMatch::Regular { key: name, re: value },
        }
    }
}

pub(crate) trait SgBackendRefConv {
    fn into_kube_httproute(self) -> HttpBackendRef;
    fn from_kube_httproute(http_backend: HttpBackendRef) -> BoxResult<Option<SgBackendRef>>;
}

impl SgBackendRefConv for SgBackendRef {
    fn into_kube_httproute(self) -> HttpBackendRef {
        let backend_inner_ref = match self.host {
            BackendHost::Host { host } => {
                let kind = match self.protocol {
                    Some(SgBackendProtocol::Https) => Some(constants::BACKEND_KIND_EXTERNAL_HTTPS.to_string()),
                    _ => Some(constants::BACKEND_KIND_EXTERNAL_HTTP.to_string()),
                };
                BackendObjectReference {
                    group: None,
                    kind,
                    name: host,
                    namespace: None,
                    port: Some(self.port),
                }
            }
            BackendHost::K8sService(k8s_param) => BackendObjectReference {
                group: None,
                kind: None,
                name: k8s_param.name,
                namespace: k8s_param.namespace,
                port: Some(self.port),
            },
            BackendHost::File { path } => todo!(),
        };
        HttpBackendRef {
            backend_ref: Some(BackendRef {
                weight: Some(self.weight),
                timeout_ms: self.timeout_ms,
                inner: backend_inner_ref,
            }),
            filters: Some(self.plugins.into_iter().filter_map(|f| f.to_http_route_filter()).collect()),
        }
    }

    fn from_kube_httproute(http_backend: HttpBackendRef) -> BoxResult<Option<SgBackendRef>> {
        http_backend
            .backend_ref
            .map(|backend| {
                let (protocol, backend_host) = if let Some(kind) = backend.inner.kind.as_ref() {
                    match kind.as_str() {
                        constants::BACKEND_KIND_SERVICE => (
                            None,
                            BackendHost::K8sService(K8sServiceData {
                                name: backend.inner.name,
                                namespace: backend.inner.namespace,
                            }),
                        ),
                        constants::BACKEND_KIND_EXTERNAL_HTTP => (Some(gateway::SgBackendProtocol::Http), BackendHost::Host { host: backend.inner.name }),
                        constants::BACKEND_KIND_EXTERNAL_HTTPS => (Some(gateway::SgBackendProtocol::Https), BackendHost::Host { host: backend.inner.name }),
                        _ => (None, BackendHost::Host { host: backend.inner.name }),
                    }
                } else {
                    (
                        None,
                        BackendHost::K8sService(K8sServiceData {
                            name: backend.inner.name,
                            namespace: backend.inner.namespace,
                        }),
                    )
                };
                let (ext_plugins, _): (Vec<_>, Vec<_>) =
                    http_backend.filters.map(|f_vec| f_vec.into_iter().partition(|f| matches!(f, HttpRouteFilter::ExtensionRef { extension_ref: _ }))).unwrap_or_default();
                Ok(SgBackendRef {
                    host: backend_host,
                    port: backend.inner.port.unwrap_or(80),
                    timeout_ms: backend.timeout_ms,
                    protocol,
                    weight: backend.weight.unwrap_or(1),
                    plugins: ext_plugins.into_iter().filter_map(PluginInstanceId::from_http_route_filter).collect(),
                })
            })
            .transpose()
    }
}

impl ToTarget for HttpSpaceroute {
    fn to_target_ref(&self) -> K8sSgFilterSpecTargetRef {
        K8sSgFilterSpecTargetRef {
            kind: SgFilterTargetKind::Httpspaceroute.into(),
            name: self.name_any(),
            namespace: self.namespace(),
        }
    }
}
