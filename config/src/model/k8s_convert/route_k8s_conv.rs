use std::collections::BTreeMap;

use gateway::SgBackendProtocol;
use http_route::SgHttpRoute;
use k8s_gateway_api::{BackendObjectReference, CommonRouteSpec, HttpHeaderMatch, HttpPathMatch, HttpQueryParamMatch, HttpRouteMatch, ParentReference};
use kube::api::ObjectMeta;

use crate::{
    constants,
    k8s_crd::{
        http_spaceroute::{self, BackendRef, HttpBackendRef, HttpRouteRule, HttpSpaceroute, HttpSpacerouteSpec},
        sg_filter::K8sSgFilterSpecTargetRef,
    },
    model::{
        gateway, helper_filter::SgSingeFilter, http_route, BackendHost, K8sServiceData, SgBackendRef, SgHttpHeaderMatch, SgHttpPathMatch, SgHttpQueryMatch, SgHttpRouteMatch,
        SgHttpRouteRule, SgRouteFilter,
    },
    BoxResult,
};

impl SgHttpRoute {
    /// Convert to HttpSpaceroute and SgSingeFilter
    /// And SgSingeFilter ref kind is 'HTTPSpaceroute'
    pub fn to_kube_httproute_spaceroute_filters(self, name: &str, namespace: &str) -> (HttpSpaceroute, Vec<SgSingeFilter>) {
        self.to_kube_httproute(name, namespace, constants::RAW_HTTP_ROUTE_KIND_SPACEROUTE)
    }

    /// Convert to HttpSpaceroute and SgSingeFilter
    /// And SgSingeFilter ref kind is 'HTTPRoute'
    pub fn to_kube_httproute_route_filters(self, name: &str, namespace: &str) -> (HttpSpaceroute, Vec<SgSingeFilter>) {
        self.to_kube_httproute(name, namespace, constants::RAW_HTTP_ROUTE_KIND_DEFAULT)
    }

    pub fn to_kube_httproute(self, name: &str, namespace: &str, self_kind: &str) -> (HttpSpaceroute, Vec<SgSingeFilter>) {
        let mut sgfilters: Vec<SgSingeFilter> = self
            .rules
            .iter()
            .flat_map(|r| {
                let mut route_filters_vec = r
                    .filters
                    .clone()
                    .into_iter()
                    .filter_map(|f| {
                        f.to_singe_filter(K8sSgFilterSpecTargetRef {
                            kind: self_kind.to_string(),
                            name: name.to_string(),
                            namespace: Some(namespace.to_string()),
                        })
                    })
                    .collect::<Vec<_>>();

                let mut b_singe_f_vec = r
                    .backends
                    .iter()
                    .flat_map(|b| {
                        b.filters
                            .iter()
                            .filter_map(|b_f| {
                                b_f.clone().to_singe_filter(K8sSgFilterSpecTargetRef {
                                    kind: self_kind.to_string(),
                                    name: name.to_string(),
                                    namespace: Some(namespace.to_string()),
                                })
                            })
                            .collect::<Vec<_>>()
                    })
                    .collect::<Vec<_>>();

                route_filters_vec.append(&mut b_singe_f_vec);
                route_filters_vec
            })
            .collect::<Vec<SgSingeFilter>>();

        sgfilters.append(
            &mut self
                .filters
                .into_iter()
                .filter_map(|f| {
                    f.to_singe_filter(K8sSgFilterSpecTargetRef {
                        kind: self_kind.to_string(),
                        name: name.to_string(),
                        namespace: Some(namespace.to_string()),
                    })
                })
                .collect::<Vec<_>>(),
        );

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
                        namespace: Some(namespace.to_string()),
                        name: self.gateway_name.clone(),
                        section_name: None,
                        port: None,
                    }]),
                },
                hostnames: self.hostnames,
                rules: Some(self.rules.into_iter().map(|r| r.into_kube_httproute()).collect::<Vec<_>>()),
            },
            status: None,
        };
        (httproute, sgfilters)
    }
}

impl SgHttpRouteRule {
    /// # to_kube_httproute
    /// `SgHttpRouteRule` to `HttpRouteRule`, include `HttpRouteFilter` and  excluding `SgFilter`.
    pub(crate) fn into_kube_httproute(self) -> HttpRouteRule {
        HttpRouteRule {
            matches: self.matches.map(|m_vec| m_vec.into_iter().flat_map(|m| m.into_kube_httproute()).collect::<Vec<_>>()),
            filters: Some(self.filters.into_iter().filter_map(|f| f.to_http_route_filter()).collect::<Vec<_>>()),
            backend_refs: Some(self.backends.into_iter().map(|b| b.into_kube_httproute()).collect::<Vec<_>>()),
            timeout_ms: self.timeout_ms,
        }
    }

    pub(crate) fn from_kube_httproute(rule: http_spaceroute::HttpRouteRule) -> BoxResult<SgHttpRouteRule> {
        Ok(SgHttpRouteRule {
            matches: rule.matches.map(|m_vec| m_vec.into_iter().map(SgHttpRouteMatch::from_kube_httproute).collect::<Vec<_>>()),
            filters: rule.filters.map(|f_vec| f_vec.into_iter().map(SgRouteFilter::from_http_route_filter).collect::<BoxResult<Vec<_>>>()).transpose()?.unwrap_or_default(),
            backends: rule
                .backend_refs
                .map(|b_vec| b_vec.into_iter().filter_map(|b| SgBackendRef::from_kube_httproute(b).transpose()).collect::<BoxResult<Vec<_>>>())
                .transpose()?
                .unwrap_or_default(),
            timeout_ms: rule.timeout_ms,
        })
    }
}

impl SgHttpRouteMatch {
    pub(crate) fn into_kube_httproute(self) -> Vec<HttpRouteMatch> {
        if let Some(method_vec) = self.method {
            method_vec
                .into_iter()
                .map(|m| HttpRouteMatch {
                    path: self.path.clone().map(|p| p.into_kube_httproute()),
                    headers: self.header.clone().map(|h_vec| h_vec.into_iter().map(|h| h.into_kube_httproute()).collect::<Vec<_>>()),
                    query_params: self.query.clone().map(|q_vec| q_vec.into_iter().map(|q| q.into_kube_httproute()).collect::<Vec<_>>()),
                    method: Some(m.0),
                })
                .collect::<Vec<_>>()
        } else {
            vec![HttpRouteMatch {
                path: self.path.map(|p| p.into_kube_httproute()),
                headers: self.header.map(|h_vec| h_vec.into_iter().map(|h| h.into_kube_httproute()).collect::<Vec<_>>()),
                query_params: self.query.map(|q_vec| q_vec.into_iter().map(|q| q.into_kube_httproute()).collect::<Vec<_>>()),
                method: None,
            }]
        }
    }

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
    pub(crate) fn into_kube_httproute(self) -> HttpPathMatch {
        match self {
            SgHttpPathMatch::Exact(value) => HttpPathMatch::Exact { value },
            SgHttpPathMatch::Prefix(value) => HttpPathMatch::PathPrefix { value },
            SgHttpPathMatch::Regular(value) => HttpPathMatch::RegularExpression { value },
        }
    }

    pub(crate) fn from_kube_httproute(path_match: HttpPathMatch) -> SgHttpPathMatch {
        match path_match {
            HttpPathMatch::Exact { value } => SgHttpPathMatch::Exact(value),
            HttpPathMatch::PathPrefix { value } => SgHttpPathMatch::Prefix(value),
            HttpPathMatch::RegularExpression { value } => SgHttpPathMatch::Regular(value),
        }
    }
}

impl SgHttpHeaderMatch {
    pub(crate) fn into_kube_httproute(self) -> HttpHeaderMatch {
        match self {
            SgHttpHeaderMatch::Exact { name, value } => HttpHeaderMatch::Exact { name, value },
            SgHttpHeaderMatch::Regular { name, re: value } => HttpHeaderMatch::RegularExpression { name, value },
        }
    }

    pub(crate) fn from_kube_httproute(header_match: HttpHeaderMatch) -> SgHttpHeaderMatch {
        match header_match {
            HttpHeaderMatch::Exact { name, value } => SgHttpHeaderMatch::Exact { name, value },
            HttpHeaderMatch::RegularExpression { name, value } => SgHttpHeaderMatch::Regular { name, re: value },
        }
    }
}

impl SgHttpQueryMatch {
    pub(crate) fn into_kube_httproute(self) -> HttpQueryParamMatch {
        match self {
            SgHttpQueryMatch::Exact { key: name, value } => HttpQueryParamMatch::Exact { name, value },
            SgHttpQueryMatch::Regular { key: name, re: value } => HttpQueryParamMatch::RegularExpression { name, value },
        }
    }

    pub(crate) fn from_kube_httproute(query_match: HttpQueryParamMatch) -> SgHttpQueryMatch {
        match query_match {
            HttpQueryParamMatch::Exact { name, value } => SgHttpQueryMatch::Exact { key: name, value },
            HttpQueryParamMatch::RegularExpression { name, value } => SgHttpQueryMatch::Regular { key: name, re: value },
        }
    }
}

impl SgBackendRef {
    pub(crate) fn into_kube_httproute(self) -> HttpBackendRef {
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
            filters: Some(self.filters.into_iter().filter_map(|f| f.to_http_route_filter()).collect()),
        }
    }

    pub(crate) fn from_kube_httproute(http_backend: HttpBackendRef) -> BoxResult<Option<SgBackendRef>> {
        http_backend
            .backend_ref
            .map(|backend| {
                let (protocol, backend_host) = if let Some(kind) = backend.inner.kind.as_ref() {
                    (
                        match kind.as_str() {
                            constants::BANCKEND_KIND_EXTERNAL_HTTP => Some(gateway::SgBackendProtocol::Http),
                            constants::BANCKEND_KIND_EXTERNAL_HTTPS => Some(gateway::SgBackendProtocol::Https),
                            _ => None,
                        },
                        BackendHost::Host { host: backend.inner.name },
                    )
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
                    filters: http_backend
                        .filters
                        .map(|f_vec| f_vec.into_iter().map(SgRouteFilter::from_http_route_filter).collect::<BoxResult<Vec<SgRouteFilter>>>())
                        .transpose()?
                        .unwrap_or_default(),
                })
            })
            .transpose()
    }
}
