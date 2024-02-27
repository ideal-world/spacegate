use std::collections::HashMap;

use k8s_gateway_api::{HttpHeader, HttpPathModifier, HttpRequestHeaderFilter, HttpRequestRedirectFilter, HttpRouteFilter, HttpUrlRewriteFilter};

use crate::{
    constants,
    k8s_crd::sg_filter::{K8sSgFilterSpecFilter, K8sSgFilterSpecTargetRef},
    model::{
        gatewayapi_support_filter::{
            SgFilterHeaderModifier, SgFilterHeaderModifierKind, SgFilterRedirect, SgFilterRewrite, SgHttpPathModifier, SgHttpPathModifierType, SG_FILTER_HEADER_MODIFIER_CODE,
            SG_FILTER_REDIRECT_CODE, SG_FILTER_REWRITE_CODE,
        },
        helper_filter::SgSingeFilter,
        SgRouteFilter,
    },
    BoxResult,
};

impl SgRouteFilter {
    /// # to_singe_filter
    /// `to_single_filter` method and [SgRouteFilter::to_http_route_filter] method both convert from
    /// `SgRouteFilter`to the k8s model. The difference lies in that `to_single_filter` only includes
    /// the k8s object of `SGFilter`, whereas `to_http_route_filter` is used to convert to `HttpRouteFilter`,
    /// a filter defined in the Gateway API.
    pub fn to_singe_filter(self, target: K8sSgFilterSpecTargetRef) -> Option<SgSingeFilter> {
        if self.code == SG_FILTER_HEADER_MODIFIER_CODE || self.code == SG_FILTER_REDIRECT_CODE || self.code == SG_FILTER_REWRITE_CODE {
            None
        } else {
            Some(SgSingeFilter {
                name: self.name,
                namespace: target.namespace.clone().unwrap_or(constants::DEFAULT_NAMESPACE.to_string()),
                filter: K8sSgFilterSpecFilter {
                    code: self.code,
                    name: None,
                    config: self.spec,
                    enable: true,
                },
                target_ref: target,
            })
        }
    }

    /// # to_http_route_filter
    /// ref [SgRouteFilter::to_singe_filter]
    pub fn to_http_route_filter(self) -> Option<HttpRouteFilter> {
        if self.code == SG_FILTER_HEADER_MODIFIER_CODE {
            if let Ok(header) = serde_json::from_value::<SgFilterHeaderModifier>(self.spec) {
                let header_filter = HttpRequestHeaderFilter {
                    set: header.sets.map(|header_map| header_map.into_iter().map(|(k, v)| HttpHeader { name: k, value: v }).collect()),
                    add: None,
                    remove: header.remove,
                };
                match header.kind {
                    SgFilterHeaderModifierKind::Request => Some(HttpRouteFilter::RequestHeaderModifier {
                        request_header_modifier: header_filter,
                    }),
                    SgFilterHeaderModifierKind::Response => Some(HttpRouteFilter::ResponseHeaderModifier {
                        response_header_modifier: header_filter,
                    }),
                }
            } else {
                None
            }
        } else if self.code == SG_FILTER_REDIRECT_CODE {
            if let Ok(redirect) = serde_json::from_value::<SgFilterRedirect>(self.spec) {
                Some(HttpRouteFilter::RequestRedirect {
                    request_redirect: HttpRequestRedirectFilter {
                        scheme: redirect.scheme,
                        hostname: redirect.hostname,
                        path: redirect.path.map(|p| p.to_http_path_modifier()),
                        port: redirect.port,
                        status_code: redirect.status_code,
                    },
                })
            } else {
                None
            }
        } else if self.code == SG_FILTER_REWRITE_CODE {
            if let Ok(rewrite) = serde_json::from_value::<SgFilterRewrite>(self.spec) {
                Some(HttpRouteFilter::URLRewrite {
                    url_rewrite: HttpUrlRewriteFilter {
                        hostname: rewrite.hostname,
                        path: rewrite.path.map(|p| p.to_http_path_modifier()),
                    },
                })
            } else {
                None
            }
        } else {
            None
        }
    }

    pub(crate) fn from_http_route_filter(route_filter: HttpRouteFilter) -> BoxResult<SgRouteFilter> {
        let process_header_modifier = |header_modifier: HttpRequestHeaderFilter, modifier_kind: SgFilterHeaderModifierKind| -> BoxResult<SgRouteFilter> {
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
                spec: serde_json::to_value(SgFilterHeaderModifier {
                    kind: modifier_kind,
                    sets: if sg_sets.is_empty() { None } else { Some(sg_sets) },
                    remove: header_modifier.remove,
                })?,
            })
        };
        let sg_filter = match route_filter {
            k8s_gateway_api::HttpRouteFilter::RequestHeaderModifier { request_header_modifier } => {
                process_header_modifier(request_header_modifier, SgFilterHeaderModifierKind::Request)?
            }
            k8s_gateway_api::HttpRouteFilter::ResponseHeaderModifier { response_header_modifier } => {
                process_header_modifier(response_header_modifier, SgFilterHeaderModifierKind::Response)?
            }
            k8s_gateway_api::HttpRouteFilter::RequestRedirect { request_redirect } => SgRouteFilter {
                code: SG_FILTER_REDIRECT_CODE.to_string(),
                name: None,
                spec: serde_json::to_value(SgFilterRedirect {
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
                spec: serde_json::to_value(SgFilterRewrite {
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

impl SgHttpPathModifier {
    pub fn to_http_path_modifier(self) -> HttpPathModifier {
        match self.kind {
            SgHttpPathModifierType::ReplaceFullPath => HttpPathModifier::ReplaceFullPath { replace_full_path: self.value },
            SgHttpPathModifierType::ReplacePrefixMatch => HttpPathModifier::ReplacePrefixMatch { replace_prefix_match: self.value },
        }
    }
}
