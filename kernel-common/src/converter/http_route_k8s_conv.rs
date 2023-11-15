use crate::converter::plugin_k8s_conv::SgSingeFilter;
use crate::helper::k8s_helper::{get_k8s_obj_unique, parse_k8s_obj_unique};
use crate::inner_model::http_route::{
    SgBackendRef, SgHttpHeaderMatch, SgHttpHeaderMatchType, SgHttpPathMatch, SgHttpPathMatchType, SgHttpQueryMatch, SgHttpQueryMatchType, SgHttpRoute, SgHttpRouteMatch,
    SgHttpRouteRule,
};
use crate::k8s_crd::http_spaceroute::{BackendRef, HttpBackendRef, HttpRouteRule, HttpSpaceroute, HttpSpacerouteSpec};
use crate::k8s_crd::sg_filter::K8sSgFilterSpecTargetRef;
use k8s_gateway_api::{BackendObjectReference, CommonRouteSpec, HttpHeaderMatch, HttpPathMatch, HttpQueryParamMatch, HttpRouteMatch, ParentReference};
use k8s_openapi::apimachinery::pkg::apis::meta::v1::ObjectMeta;
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
                                    .map(|f| {
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
                                                    .map(|b_f| {
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
                        .map(|f| {
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

    pub async fn from_kube_gateway(httproute: HttpSpaceroute) -> TardisResult<SgHttpRoute> {
        //todo
        Ok(SgHttpRoute {
            name: get_k8s_obj_unique(&httproute),
            gateway_name: "".to_string(),
            hostnames: None,
            filters: None,
            rules: None,
        })
    }
}

impl SgHttpRouteRule {
    /// `SgHttpRouteRule` to `HttpRouteRule`, excluding SgFilter.
    pub(crate) fn to_kube_httproute(self) -> HttpRouteRule {
        HttpRouteRule {
            matches: self.matches.map(|m_vec| m_vec.into_iter().map(|m| m.to_kube_httproute()).flatten().collect::<Vec<_>>()),
            filters: self.filters.map(|f_vec| f_vec.into_iter().filter_map(|f| f.to_http_route_filter()).collect::<Vec<_>>()),
            backend_refs: self.backends.map(|b_vec| b_vec.into_iter().map(|b| b.to_kube_httproute()).collect::<Vec<_>>()),
            timeout_ms: self.timeout_ms,
        }
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
}

impl SgHttpPathMatch {
    pub(crate) fn to_kube_httproute(self) -> HttpPathMatch {
        match self.kind {
            SgHttpPathMatchType::Exact => HttpPathMatch::Exact { value: self.value },
            SgHttpPathMatchType::Prefix => HttpPathMatch::PathPrefix { value: self.value },
            SgHttpPathMatchType::Regular => HttpPathMatch::RegularExpression { value: self.value },
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
}

impl SgBackendRef {
    //todo excluding SgFilter.
    pub(crate) fn to_kube_httproute(self) -> HttpBackendRef {
        HttpBackendRef {
            backend_ref: Some(BackendRef {
                weight: None,
                timeout_ms: None,
                inner: BackendObjectReference {
                    group: None,
                    kind: None,
                    name: self.name_or_host,
                    namespace: self.namespace,
                    port: None,
                },
            }),
            filters: None,
        }
    }
}
