use std::{cmp::Ordering, collections::BTreeMap};

use futures_util::future::join_all;
use gateway::SgBackendProtocol;
use http_route::SgHttpRoute;
use k8s_gateway_api::{
    BackendObjectReference, CommonRouteSpec, HttpHeader, HttpHeaderMatch, HttpPathMatch, HttpPathModifier, HttpQueryParamMatch, HttpRequestHeaderFilter, HttpRouteFilter,
    HttpRouteMatch, HttpUrlRewriteFilter, ParentReference, RouteParentStatus, RouteStatus,
};
use kube::{api::ObjectMeta, ResourceExt};
use spacegate_model::{
    constants::GATEWAY_CONTROLLER_NAME,
    ext::k8s::{
        crd::{http_spaceroute::HttpSpacerouteStatus, sg_filter::K8sSgFilterSpecTargetRef},
        helper_struct::{BackendObjectRefKind, SgTargetKind},
    },
    PluginInstanceId,
};

use crate::{
    constants,
    ext::k8s::{
        crd::http_spaceroute::{self, BackendRef, HttpBackendRef, HttpRouteRule, HttpSpaceroute, HttpSpacerouteSpec},
        helper_struct::SgSingeFilter,
    },
    gateway, http_route,
    service::k8s::K8s,
    BackendHost, BoxResult, K8sServiceData, PluginConfig, SgBackendRef, SgHttpHeaderMatch, SgHttpPathMatch, SgHttpQueryMatch, SgHttpRouteMatch, SgHttpRouteRule,
};

use super::{filter_k8s_conv::PluginIdConv as _, ToTarget};
pub(crate) trait SgHttpRouteConv {
    /// Convert to HttpSpaceroute and SgSingeFilter
    fn to_kube_httproute(self, gateway_name: &str, name: &str, gateway_namespace: &str) -> (HttpSpaceroute, Vec<PluginInstanceId>);
}

impl SgHttpRouteConv for SgHttpRoute {
    fn to_kube_httproute(self, gateway_name: &str, name: &str, gateway_namespace: &str) -> (HttpSpaceroute, Vec<PluginInstanceId>) {
        let gateway_ref = ParentReference {
            group: None,
            kind: Some(SgTargetKind::Gateway.into()),
            namespace: Some(gateway_namespace.to_string()),
            name: gateway_name.to_string(),
            section_name: None,
            port: None,
        };
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
                    parent_refs: Some(vec![gateway_ref.clone()]),
                },
                hostnames: self.hostnames,
                rules: Some(self.rules.into_iter().map(|r| r.into_kube_httproute()).collect::<Vec<_>>()),
            },
            status: Some(HttpSpacerouteStatus {
                inner: RouteStatus {
                    parents: vec![RouteParentStatus {
                        parent_ref: gateway_ref,
                        controller_name: GATEWAY_CONTROLLER_NAME.to_string(),
                        conditions: Vec::new(),
                    }],
                },
            }),
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
                let mut m: SgHttpRouteMatch = SgHttpRouteMatch::from_kube_httproute(match_);
                if legacy_plugins.iter().filter(|p| matches!(p, HttpRouteFilter::URLRewrite { url_rewrite: _ })).count() > 1 {
                    return Err("url_rewrite can only have one in each rule".into());
                } else if let Some(url_rewrite) = legacy_plugins.iter().find(|p| matches!(p, HttpRouteFilter::URLRewrite { url_rewrite: _ })) {
                    m.path = m.path.map(|m_p| match url_rewrite {
                        HttpRouteFilter::URLRewrite { url_rewrite } => {
                            if let Some(rewrite_path) = &url_rewrite.path {
                                match &m_p {
                                    SgHttpPathMatch::Exact { value, replace: _ } => match &rewrite_path {
                                        HttpPathModifier::ReplaceFullPath { replace_full_path } => SgHttpPathMatch::Exact {
                                            value: value.clone(),
                                            replace: Some(replace_full_path.clone()),
                                        },
                                        _ => m_p,
                                    },
                                    SgHttpPathMatch::Prefix { value, replace: _ } => match rewrite_path {
                                        HttpPathModifier::ReplacePrefixMatch { replace_prefix_match } => SgHttpPathMatch::Exact {
                                            value: value.clone(),
                                            replace: Some(replace_prefix_match.clone()),
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

    /// Returns (matches, filters)
    /// path 和 header 对应插件的关系是:
    ///
    /// e.g
    ///
    /// | matches | plugins |
    /// | --- | --- |
    /// | path_match1 | Some(url_rewrite1) |
    /// | path_match2 | Some(url_rewrite2) |
    /// | path_match3 | None |
    /// | header1 | Some(request_header_modifier1) |
    /// | header2 | Some(request_header_modifier2) |
    /// | header3 | None |
    /// | ~~header4,header5~~（not support） | ~~Some(request_header_modifier3),None~~ |
    /// | path_match4,header6 | Some(url_rewrite3),None |
    /// | path_match5,header7 | None,Some(request_header_modifier4) |
    ///
    /// result ==>
    ///
    /// matches_vec:[path_match1, path_match2, path_match3, header1, header2, header3, (path_match4,header6), (path_match5,header7)]
    /// filter_vec:[url_rewrite1,url_rewrite2,request_header_modifier1,request_header_modifier2,url_rewrite3,request_header_modifier4]
    ///
    ///
    fn into_kube_httproute(self) -> (Vec<HttpRouteMatch>, Vec<HttpRouteFilter>);
}
impl SgHttpRouteMatchConv for SgHttpRouteMatch {
    fn into_kube_httproute(self) -> (Vec<HttpRouteMatch>, Vec<HttpRouteFilter>) {
        // todo: not complete
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

                    let (header_path, header_plugins) = self
                        .header
                        .clone()
                        .map(|hs| {
                            let mut headers_p = hs
                                .into_iter()
                                .map(|h| {
                                    let (path, plugin) = h.into_kube_httproute();
                                    (Some(path), plugin)
                                })
                                .collect::<Vec<_>>();
                            headers_p.sort_by(|a, b| {
                                if a.1.is_some() && b.1.is_some() {
                                    Ordering::Equal
                                } else if a.1.is_some() {
                                    Ordering::Less
                                } else {
                                    Ordering::Greater
                                }
                            });

                            let (a, b): (Vec<_>, Vec<_>) = headers_p.into_iter().unzip();
                            (a, b)
                        })
                        .unwrap_or((vec![], vec![]));
                    let header_paths: Vec<_> = header_path.into_iter().filter_map(|x| x).collect();

                    (
                        HttpRouteMatch {
                            path: path,
                            headers: if header_paths.is_empty() { None } else { Some(header_paths) },
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
                    //todo
                    headers: None,
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
    fn into_kube_httproute(self) -> (HttpHeaderMatch, Option<HttpRouteFilter>);
}

impl SgHttpHeaderMatchConv for SgHttpHeaderMatch {
    fn into_kube_httproute(self) -> (HttpHeaderMatch, Option<HttpRouteFilter>) {
        match self {
            SgHttpHeaderMatch::Exact { name, value, replace } => (
                HttpHeaderMatch::Exact { name: name.clone(), value },
                replace.map(|r| HttpRouteFilter::RequestHeaderModifier {
                    request_header_modifier: HttpRequestHeaderFilter {
                        set: Some(vec![HttpHeader { name, value: r }]),
                        add: None,
                        remove: None,
                    },
                }),
            ),
            SgHttpHeaderMatch::RegExp { name, re, replace: _ } => {
                tracing::warn!("[{name} {re}]RegExp trype replace is not supported yet in kube:");
                (
                    HttpHeaderMatch::RegularExpression { name, value: re },
                    // not supported yet
                    None,
                )
            }
        }
    }

    fn from_kube_httproute(header_match: HttpHeaderMatch) -> SgHttpHeaderMatch {
        match header_match {
            HttpHeaderMatch::Exact { name, value } => SgHttpHeaderMatch::Exact { name, value, replace: None },
            HttpHeaderMatch::RegularExpression { name, value } => SgHttpHeaderMatch::RegExp { name, re: value, replace: None },
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
                    Some(SgBackendProtocol::Https) => BackendObjectRefKind::ExternalHttps.into(),
                    _ => BackendObjectRefKind::ExternalHttp.into(),
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
                kind: BackendObjectRefKind::Service.into(),
                name: k8s_param.name,
                namespace: k8s_param.namespace,
                port: Some(self.port),
            },
            BackendHost::File { path } => BackendObjectReference {
                group: None,
                kind: BackendObjectRefKind::File.into(),
                name: path,
                namespace: None,
                port: None,
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
                    match kind.to_string().into() {
                        BackendObjectRefKind::Service => (
                            None,
                            BackendHost::K8sService(K8sServiceData {
                                name: backend.inner.name,
                                namespace: backend.inner.namespace,
                            }),
                        ),
                        BackendObjectRefKind::ExternalHttp => (Some(gateway::SgBackendProtocol::Http), BackendHost::Host { host: backend.inner.name }),
                        BackendObjectRefKind::ExternalHttps => (Some(gateway::SgBackendProtocol::Https), BackendHost::Host { host: backend.inner.name }),
                        BackendObjectRefKind::File => (None, BackendHost::File { path: backend.inner.name }),
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
            kind: SgTargetKind::Httpspaceroute.into(),
            name: self.name_any(),
            namespace: self.namespace(),
        }
    }
}
