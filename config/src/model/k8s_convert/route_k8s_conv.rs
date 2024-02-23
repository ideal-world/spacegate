use k8s_gateway_api::{HttpHeaderMatch, HttpPathMatch, HttpQueryParamMatch, HttpRouteMatch};

use crate::{constants, k8s_crd::http_spaceroute::{self, HttpBackendRef}, model::{gateway, http_route, BackendHost, K8sServiceData, SgBackendRef, SgHttpHeaderMatch, SgHttpPathMatch, SgHttpQueryMatch, SgHttpRouteMatch, SgHttpRouteRule, SgRouteFilter}, BoxError};

impl SgHttpRouteRule {
    pub(crate) fn from_kube_httproute(rule: http_spaceroute::HttpRouteRule) -> Result<SgHttpRouteRule, BoxError> {
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
            HttpQueryParamMatch::Exact { name, value } => SgHttpQueryMatch::Exact { key: name, value },
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
                        constants::BANCKEND_KIND_EXTERNAL_HTTP => Some(gateway::SgBackendProtocol::Http),
                        constants::BANCKEND_KIND_EXTERNAL_HTTPS => Some(gateway::SgBackendProtocol::Https),
                        _ => None,
                    }
                } else {
                    None
                };
                Ok(SgBackendRef {
                    host: BackendHost::K8sService(K8sServiceData{ name:backend.inner.name, namespace: backend.inner.namespace }),
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