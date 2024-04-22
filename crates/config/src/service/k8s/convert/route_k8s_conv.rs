use std::collections::BTreeMap;

use futures_util::future::join_all;
use gateway::SgBackendProtocol;
use http_route::SgHttpRoute;
use hyper::client;
use k8s_gateway_api::{
    BackendObjectReference, CommonRouteSpec, HttpHeaderMatch, HttpPathMatch, HttpPathModifier, HttpQueryParamMatch, HttpRouteFilter, HttpRouteMatch, HttpUrlRewriteFilter,
    ParentReference,
};
use kube::api::ObjectMeta;
use spacegate_model::PluginInstanceId;

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

use super::filter_k8s_conv::PluginIdConv as _;
pub(crate) trait SgHttpRouteConv {
    /// Convert to HttpSpaceroute and SgSingeFilter
    /// And SgSingeFilter ref kind is 'HTTPRoute'
    async fn to_kube_httproute_route_filters(self, gateway_name: &str, name: &str, client: &K8s) -> (HttpSpaceroute, Vec<PluginInstanceId>);
    /// Convert to HttpSpaceroute and SgSingeFilter
    /// And SgSingeFilter ref kind is 'HTTPSpaceroute'
    async fn to_kube_httproute_spaceroute_filters(self, gateway_name: &str, name: &str, client: &K8s) -> (HttpSpaceroute, Vec<PluginInstanceId>);

    async fn to_kube_httproute(self, gateway_name: &str, name: &str, client: &K8s, self_kind: &str) -> (HttpSpaceroute, Vec<PluginInstanceId>);
}

impl SgHttpRouteConv for SgHttpRoute {
    async fn to_kube_httproute_spaceroute_filters(self, gateway_name: &str, name: &str, client: &K8s) -> (HttpSpaceroute, Vec<PluginInstanceId>) {
        self.to_kube_httproute(gateway_name, name, client, constants::RAW_HTTP_ROUTE_KIND_SPACEROUTE).await
    }

    async fn to_kube_httproute_route_filters(self, gateway_name: &str, name: &str, client: &K8s) -> (HttpSpaceroute, Vec<PluginInstanceId>) {
        self.to_kube_httproute(gateway_name, name, client, constants::RAW_HTTP_ROUTE_KIND_DEFAULT).await
    }

    async fn to_kube_httproute(self, gateway_name: &str, name: &str, client: &K8s, self_kind: &str) -> (HttpSpaceroute, Vec<PluginInstanceId>) {
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
    async fn into_kube_httproute(self, client: &K8s) -> HttpRouteRule;
    fn from_kube_httproute(rule: http_spaceroute::HttpRouteRule) -> BoxResult<SgHttpRouteRule>;
}

impl SgHttpRouteRuleConv for SgHttpRouteRule {
    async fn into_kube_httproute(self, client: &K8s) -> HttpRouteRule {
        let (matches, plugins): (Option<Vec<HttpRouteMatch>>, Option<Vec<HttpRouteFilter>>) = self
            .matches
            .map(|m_vec| {
                let (a, b): (Vec<_>, Vec<_>) = m_vec.into_iter().map(|m| m.into_kube_httproute()).unzip();
                (Some(a.into_iter().flatten().collect()), Some(b.into_iter().flatten().collect()))
            })
            .unwrap_or_default();
        HttpRouteRule {
            matches: matches,
            filters: plugins,
            backend_refs: Some(self.backends.into_iter().map(|b| b.into_kube_httproute()).collect::<Vec<_>>()),
            timeout_ms: self.timeout_ms,
        }
    }

    fn from_kube_httproute(rule: http_spaceroute::HttpRouteRule) -> BoxResult<SgHttpRouteRule> {
        Ok(SgHttpRouteRule {
            matches: rule.matches.map(|m_vec| m_vec.into_iter().map(SgHttpRouteMatch::from_kube_httproute).collect::<Vec<_>>()),
            plugins: rule.filters.map(|f_vec| f_vec.into_iter().map(PluginConfig::from_http_route_filter).collect::<BoxResult<Vec<_>>>()).transpose()?.unwrap_or_default(),
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
        let (matchs, plugins) = if let Some(method_vec) = self.method {
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
        (matchs, plugins.into_iter().filter_map(|x| x).collect())
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
                    Some(SgBackendProtocol::Https) => Some(constants::BANCKEND_KIND_EXTERNAL_HTTPS.to_string()),
                    _ => Some(constants::BANCKEND_KIND_EXTERNAL_HTTP.to_string()),
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
                        constants::BANCKEND_KIND_SERVICE => (
                            None,
                            BackendHost::K8sService(K8sServiceData {
                                name: backend.inner.name,
                                namespace: backend.inner.namespace,
                            }),
                        ),
                        constants::BANCKEND_KIND_EXTERNAL_HTTP => (Some(gateway::SgBackendProtocol::Http), BackendHost::Host { host: backend.inner.name }),
                        constants::BANCKEND_KIND_EXTERNAL_HTTPS => (Some(gateway::SgBackendProtocol::Https), BackendHost::Host { host: backend.inner.name }),
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
                Ok(SgBackendRef {
                    host: backend_host,
                    port: backend.inner.port.unwrap_or(80),
                    timeout_ms: backend.timeout_ms,
                    protocol,
                    weight: backend.weight.unwrap_or(1),
                    plugins: http_backend
                        .filters
                        .map(|f_vec| f_vec.into_iter().map(PluginConfig::from_http_route_filter).collect::<BoxResult<Vec<PluginConfig>>>())
                        .transpose()?
                        .unwrap_or_default(),
                })
            })
            .transpose()
    }
}