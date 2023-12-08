use crate::constants::{self, BANCKEND_KIND_EXTERNAL, BANCKEND_KIND_EXTERNAL_HTTP, BANCKEND_KIND_EXTERNAL_HTTPS};
use crate::converter::plugin_k8s_conv::SgSingeFilter;
use crate::helper::k8s_helper::{get_k8s_obj_unique, parse_k8s_obj_unique};
use crate::inner_model::gateway::SgProtocol;
use crate::inner_model::http_route::{
    SgBackendRef, SgHttpHeaderMatch, SgHttpHeaderMatchType, SgHttpPathMatch, SgHttpPathMatchType, SgHttpQueryMatch, SgHttpQueryMatchType, SgHttpRoute, SgHttpRouteMatch,
    SgHttpRouteRule,
};
use crate::inner_model::plugin_filter::SgRouteFilter;
use crate::k8s_crd::http_spaceroute::{BackendRef, HttpBackendRef, HttpRouteRule, HttpSpaceroute, HttpSpacerouteSpec};
use crate::k8s_crd::sg_filter::K8sSgFilterSpecTargetRef;
use k8s_gateway_api::{BackendObjectReference, CommonRouteSpec, HttpHeaderMatch, HttpPathMatch, HttpQueryParamMatch, HttpRouteMatch, ParentReference};
use k8s_openapi::apimachinery::pkg::apis::meta::v1::ObjectMeta;
use kube::ResourceExt;
use tardis::basic::result::TardisResult;

impl SgHttpRoute {
    pub fn to_kube_httproute(self) -> (HttpSpaceroute, Vec<SgSingeFilter>) {
        let (namespace, raw_name) = parse_k8s_obj_unique(&self.name);

        let (gateway_namespace, gateway_name) = parse_k8s_obj_unique(&self.gateway_name);

        let mut sgfilters: Vec<SgSingeFilter> = self
            .rules
            .as_ref()
            .map(|r_vec| {
                r_vec
                    .iter()
                    .map(|r| {
                        let mut route_filters_vec = r
                            .filters
                            .clone()
                            .map(|filters| {
                                filters
                                    .into_iter()
                                    .filter_map(|f| {
                                        f.to_singe_filter(K8sSgFilterSpecTargetRef {
                                            kind: "HTTPSpaceroute".to_string(),
                                            name: self.name.clone(),
                                            namespace: Some(namespace.to_string()),
                                        })
                                    })
                                    .collect::<Vec<_>>()
                            })
                            .unwrap_or_default();

                        let mut b_singe_f_vec = r
                            .backends
                            .iter()
                            .map(|b_vec| {
                                b_vec
                                    .iter()
                                    .map(|b| {
                                        b.filters
                                            .as_ref()
                                            .map(|b_f_vec| {
                                                b_f_vec
                                                    .iter()
                                                    .filter_map(|b_f| {
                                                        b_f.clone().to_singe_filter(K8sSgFilterSpecTargetRef {
                                                            kind: "HTTPSpaceroute".to_string(),
                                                            name: self.name.clone(),
                                                            namespace: Some(namespace.to_string()),
                                                        })
                                                    })
                                                    .collect::<Vec<_>>()
                                            })
                                            .unwrap_or_default()
                                    })
                                    .flatten()
                                    .collect::<Vec<_>>()
                            })
                            .flatten()
                            .collect::<Vec<_>>();

                        route_filters_vec.append(&mut b_singe_f_vec);
                        route_filters_vec
                    })
                    .flatten()
                    .collect::<Vec<SgSingeFilter>>()
            })
            .unwrap_or_default();

        sgfilters.append(
            &mut self
                .filters
                .map(|filters| {
                    filters
                        .into_iter()
                        .filter_map(|f| {
                            f.to_singe_filter(K8sSgFilterSpecTargetRef {
                                kind: "HTTPSpaceroute".to_string(),
                                name: self.name.clone(),
                                namespace: Some(namespace.to_string()),
                            })
                        })
                        .collect::<Vec<_>>()
                })
                .unwrap_or_default(),
        );

        let httproute = HttpSpaceroute {
            metadata: ObjectMeta {
                labels: None,
                name: Some(raw_name),
                owner_references: None,
                self_link: None,
                ..Default::default()
            },
            spec: HttpSpacerouteSpec {
                inner: CommonRouteSpec {
                    parent_refs: Some(vec![ParentReference {
                        group: None,
                        kind: Some("Gateway".to_string()),
                        namespace: Some(gateway_namespace),
                        name: gateway_name,
                        section_name: None,
                        port: None,
                    }]),
                },
                hostnames: self.hostnames,
                rules: self.rules.map(|r_vec| r_vec.into_iter().map(|r| r.to_kube_httproute()).collect::<Vec<_>>()),
            },
            status: None,
        };
        (httproute, sgfilters)
    }

    pub async fn from_kube_httpspaceroute(client_name: &str, httproute: HttpSpaceroute) -> TardisResult<SgHttpRoute> {
        let kind = if let Some(kind) = httproute.annotations().get(constants::RAW_HTTP_ROUTE_KIND) {
            kind
        } else {
            constants::RAW_HTTP_ROUTE_KIND_SPACEROUTE
        };
        Ok(SgHttpRoute {
            name: get_k8s_obj_unique(&httproute),
            gateway_name: httproute.spec.inner.parent_refs.clone().unwrap_or_default().get(0).map(|x| x.name.clone()).unwrap_or_default(),
            hostnames: httproute.spec.hostnames.clone(),
            filters: SgRouteFilter::from_crd_filters(client_name, kind, &httproute.metadata.name, &httproute.metadata.namespace).await?,
            rules: httproute.spec.rules.map(|r_vec| r_vec.into_iter().map(|r| SgHttpRouteRule::from_kube_httproute(r)).collect::<TardisResult<Vec<_>>>()).transpose()?,
        })
    }
}

impl SgHttpRouteRule {
    /// # to_kube_httproute
    /// `SgHttpRouteRule` to `HttpRouteRule`, include `HttpRouteFilter` and  excluding `SgFilter`.
    pub(crate) fn to_kube_httproute(self) -> HttpRouteRule {
        HttpRouteRule {
            matches: self.matches.map(|m_vec| m_vec.into_iter().map(|m| m.to_kube_httproute()).flatten().collect::<Vec<_>>()),
            filters: self.filters.map(|f_vec| f_vec.into_iter().filter_map(|f| f.to_http_route_filter()).collect::<Vec<_>>()),
            backend_refs: self.backends.map(|b_vec| b_vec.into_iter().map(|b| b.to_kube_httproute()).collect::<Vec<_>>()),
            timeout_ms: self.timeout_ms,
        }
    }

    pub(crate) fn from_kube_httproute(httproute_rule: HttpRouteRule) -> TardisResult<SgHttpRouteRule> {
        Ok(SgHttpRouteRule {
            matches: httproute_rule.matches.map(|m_vec| m_vec.into_iter().map(|m| SgHttpRouteMatch::from_kube_httproute(m)).collect::<Vec<_>>()),
            filters: httproute_rule.filters.map(|f_vec| f_vec.into_iter().map(|f| SgRouteFilter::from_http_route_filter(f)).collect::<TardisResult<Vec<_>>>()).transpose()?,
            backends: httproute_rule
                .backend_refs
                .map(|b_vec| b_vec.into_iter().filter_map(|b| SgBackendRef::from_kube_httproute(b).transpose()).collect::<TardisResult<Vec<_>>>())
                .transpose()?,
            timeout_ms: httproute_rule.timeout_ms,
        })
    }
}

impl SgHttpRouteMatch {
    pub(crate) fn to_kube_httproute(self) -> Vec<HttpRouteMatch> {
        if let Some(method_vec) = self.method {
            method_vec
                .into_iter()
                .map(|m| HttpRouteMatch {
                    path: self.path.clone().map(|p| p.to_kube_httproute()),
                    headers: self.header.clone().map(|h_vec| h_vec.into_iter().map(|h| h.to_kube_httproute()).collect::<Vec<_>>()),
                    query_params: self.query.clone().map(|q_vec| q_vec.into_iter().map(|q| q.to_kube_httproute()).collect::<Vec<_>>()),
                    method: Some(m),
                })
                .collect::<Vec<_>>()
        } else {
            vec![HttpRouteMatch {
                path: self.path.map(|p| p.to_kube_httproute()),
                headers: self.header.map(|h_vec| h_vec.into_iter().map(|h| h.to_kube_httproute()).collect::<Vec<_>>()),
                query_params: self.query.map(|q_vec| q_vec.into_iter().map(|q| q.to_kube_httproute()).collect::<Vec<_>>()),
                method: None,
            }]
        }
    }
    pub(crate) fn from_kube_httproute(route_match: HttpRouteMatch) -> SgHttpRouteMatch {
        SgHttpRouteMatch {
            method: route_match.method.map(|m_vec| vec![m_vec]),
            path: route_match.path.map(|p| SgHttpPathMatch::from_kube_httproute(p)),
            header: route_match.headers.map(|h_vec| h_vec.into_iter().map(|h| SgHttpHeaderMatch::from_kube_httproute(h)).collect::<Vec<_>>()),
            query: route_match.query_params.map(|q_vec| q_vec.into_iter().map(|q| SgHttpQueryMatch::from_kube_httproute(q)).collect::<Vec<_>>()),
        }
    }
}

impl SgHttpPathMatch {
    pub(crate) fn to_kube_httproute(self) -> HttpPathMatch {
        match self.kind {
            SgHttpPathMatchType::Exact => HttpPathMatch::Exact { value: self.value },
            SgHttpPathMatchType::Prefix => HttpPathMatch::PathPrefix { value: self.value },
            SgHttpPathMatchType::Regular => HttpPathMatch::RegularExpression { value: self.value },
        }
    }
    pub(crate) fn from_kube_httproute(path_match: HttpPathMatch) -> SgHttpPathMatch {
        match path_match {
            HttpPathMatch::Exact { value } => SgHttpPathMatch {
                kind: SgHttpPathMatchType::Exact,
                value,
            },
            HttpPathMatch::PathPrefix { value } => SgHttpPathMatch {
                kind: SgHttpPathMatchType::Prefix,
                value,
            },
            HttpPathMatch::RegularExpression { value } => SgHttpPathMatch {
                kind: SgHttpPathMatchType::Regular,
                value,
            },
        }
    }
}

impl SgHttpHeaderMatch {
    pub(crate) fn to_kube_httproute(self) -> HttpHeaderMatch {
        match self.kind {
            SgHttpHeaderMatchType::Exact => HttpHeaderMatch::Exact {
                name: self.name,
                value: self.value,
            },
            SgHttpHeaderMatchType::Regular => HttpHeaderMatch::RegularExpression {
                name: self.name,
                value: self.value,
            },
        }
    }
    pub(crate) fn from_kube_httproute(header_match: HttpHeaderMatch) -> SgHttpHeaderMatch {
        match header_match {
            HttpHeaderMatch::Exact { name, value } => SgHttpHeaderMatch {
                kind: SgHttpHeaderMatchType::Exact,
                name,
                value,
            },
            HttpHeaderMatch::RegularExpression { name, value } => SgHttpHeaderMatch {
                kind: SgHttpHeaderMatchType::Regular,
                name,
                value,
            },
        }
    }
}

impl SgHttpQueryMatch {
    pub(crate) fn to_kube_httproute(self) -> HttpQueryParamMatch {
        match self.kind {
            SgHttpQueryMatchType::Exact => HttpQueryParamMatch::Exact {
                name: self.name,
                value: self.value,
            },
            SgHttpQueryMatchType::Regular => HttpQueryParamMatch::RegularExpression {
                name: self.name,
                value: self.value,
            },
        }
    }
    pub(crate) fn from_kube_httproute(query_match: HttpQueryParamMatch) -> SgHttpQueryMatch {
        match query_match {
            HttpQueryParamMatch::Exact { name, value } => SgHttpQueryMatch {
                kind: SgHttpQueryMatchType::Exact,
                name,
                value,
            },
            HttpQueryParamMatch::RegularExpression { name, value } => SgHttpQueryMatch {
                kind: SgHttpQueryMatchType::Regular,
                name,
                value,
            },
        }
    }
}

impl SgBackendRef {
    pub(crate) fn to_kube_httproute(self) -> HttpBackendRef {
        let kind = if self.namespace.is_none() {
            match self.protocol {
                Some(SgProtocol::Http) => Some(BANCKEND_KIND_EXTERNAL_HTTP.to_string()),
                Some(SgProtocol::Https) => Some(BANCKEND_KIND_EXTERNAL_HTTPS.to_string()),
                Some(SgProtocol::Ws) => Some(BANCKEND_KIND_EXTERNAL.to_string()),
                Some(SgProtocol::Wss) => Some(BANCKEND_KIND_EXTERNAL.to_string()),
                _ => None,
            }
        } else {
            None
        };

        HttpBackendRef {
            backend_ref: Some(BackendRef {
                weight: self.weight,
                timeout_ms: self.timeout_ms,
                inner: BackendObjectReference {
                    group: None,
                    kind,
                    name: self.name_or_host,
                    namespace: self.namespace,
                    port: Some(self.port),
                },
            }),
            filters: self.filters.map(|f_vec| f_vec.into_iter().filter_map(|f| f.to_http_route_filter()).collect()),
        }
    }

    pub(crate) fn from_kube_httproute(http_backend: HttpBackendRef) -> TardisResult<Option<SgBackendRef>> {
        http_backend
            .backend_ref
            .map(|backend| {
                let protocol = if let Some(kind) = backend.inner.kind.as_ref() {
                    match kind.as_str() {
                        BANCKEND_KIND_EXTERNAL_HTTP => Some(SgProtocol::Http),
                        BANCKEND_KIND_EXTERNAL_HTTPS => Some(SgProtocol::Https),
                        BANCKEND_KIND_EXTERNAL => Some(SgProtocol::Ws),
                        _ => None,
                    }
                } else {
                    None
                };
                return Ok(SgBackendRef {
                    name_or_host: backend.inner.name,
                    namespace: backend.inner.namespace,
                    port: backend.inner.port.unwrap_or(80),
                    timeout_ms: backend.timeout_ms,
                    protocol,
                    weight: backend.weight,
                    filters: http_backend
                        .filters
                        .map(|f_vec| f_vec.into_iter().map(|f| SgRouteFilter::from_http_route_filter(f)).collect::<TardisResult<Vec<SgRouteFilter>>>())
                        .transpose()?,
                });
            })
            .transpose()
    }
}
