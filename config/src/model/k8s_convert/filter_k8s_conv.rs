use std::collections::HashMap;

use k8s_gateway_api::{HttpRequestHeaderFilter, HttpRouteFilter};

use crate::{model::{gatewayapi_support_filter::{SgFilterHeaderModifier, SgFilterHeaderModifierKind, SgFilterRedirect, SgFilterRewrite, SgHttpPathModifier, SgHttpPathModifierType, SG_FILTER_HEADER_MODIFIER_CODE, SG_FILTER_REDIRECT_CODE, SG_FILTER_REWRITE_CODE}, SgRouteFilter}, BoxError, BoxResult};

impl SgRouteFilter {
    pub(crate) fn from_http_route_filter(route_filter: HttpRouteFilter) -> BoxResult<SgRouteFilter> {
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