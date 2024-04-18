use std::collections::HashMap;

use k8s_gateway_api::{HttpHeader, HttpPathModifier, HttpRequestHeaderFilter, HttpRequestRedirectFilter, HttpRouteFilter, HttpUrlRewriteFilter};
use kube::Api;
use spacegate_model::ext::k8s::crd::sg_filter::SgFilter;

use crate::{
    ext::k8s::{
        crd::sg_filter::{K8sSgFilterSpecFilter, K8sSgFilterSpecTargetRef},
        helper_filter::SgSingeFilter,
    },
    plugin::{
        gatewayapi_support_filter::{
            SgFilterHeaderModifier, SgFilterHeaderModifierKind, SgFilterRedirect, SgFilterRewrite, SgHttpPathModifier, SgHttpPathModifierType, SG_FILTER_HEADER_MODIFIER_CODE,
            SG_FILTER_REDIRECT_CODE, SG_FILTER_REWRITE_CODE,
        },
        PluginConfig,
    },
    service::{self, k8s::K8s},
    BoxResult, PluginInstanceId, PluginInstanceName,
};

pub(crate) trait PluginIdConv {
    fn from_http_route_filter(route_filter: HttpRouteFilter) -> BoxResult<PluginConfig>;

    /// # to_singe_filter
    /// `to_single_filter` method and [SgRouteFilter::to_http_route_filter] method both convert from
    /// `SgRouteFilter`to the k8s model. The difference lies in that `to_single_filter` only includes
    /// the k8s object of `SGFilter`, whereas `to_http_route_filter` is used to convert to `HttpRouteFilter`,
    /// a filter defined in the Gateway API.
    fn to_singe_filter(&self, value: serde_json::Value, target: Option<K8sSgFilterSpecTargetRef>, namespace: &str) -> Option<SgSingeFilter>;

    /// # to_http_route_filter
    /// ref [SgRouteFilter::to_singe_filter]
    async fn to_http_route_filter(self, client: &K8s) -> Option<HttpRouteFilter>;

    async fn add_filter_target(&self, target: K8sSgFilterSpecTargetRef, client: &K8s);

    /// mix of [SgRouteFilter::to_singe_filter] and [SgRouteFilter::to_http_route_filter]
    /// PluginInstanceId can be converted into `SgRouteFilter` or `HttpRouteFilter`
    async fn to_route_filter_or_add_filter_target(&self, target: K8sSgFilterSpecTargetRef, client: &K8s) -> Option<HttpRouteFilter>;
}

impl PluginIdConv for PluginInstanceId {
    fn to_singe_filter(&self, value: serde_json::Value, target: Option<K8sSgFilterSpecTargetRef>, namespace: &str) -> Option<SgSingeFilter> {
        match self.name.clone() {
            PluginInstanceName::Anon { uid: _ } => None,
            PluginInstanceName::Named { name } => Some(SgSingeFilter {
                name: Some(name.clone()),
                namespace: namespace.to_owned(),
                filter: K8sSgFilterSpecFilter {
                    code: self.code.to_string(),
                    name: Some(name),
                    config: value,
                    enable: true,
                },
                target_ref: target,
            }),
            PluginInstanceName::Mono => None,
        }
    }

    async fn to_http_route_filter(self, client: &service::k8s::K8s) -> Option<HttpRouteFilter> {
        let filter_api: Api<SgFilter> = client.get_namespace_api();
        if let Ok(filter) = filter_api.get(&self.name.to_string()).await {
            if let Some(plugin) = filter.spec.filters.iter().find(|f| f.code == self.code && f.name == Some(self.name.to_string())) {
                let value = plugin.config.clone();
                if self.code == SG_FILTER_HEADER_MODIFIER_CODE {
                    if let Ok(header) = serde_json::from_value::<SgFilterHeaderModifier>(value) {
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
                    if let Ok(redirect) = serde_json::from_value::<SgFilterRedirect>(value) {
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
                    if let Ok(rewrite) = serde_json::from_value::<SgFilterRewrite>(value) {
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
            } else {
                None
            }
        } else {
            None
        }
    }

    fn from_http_route_filter(route_filter: HttpRouteFilter) -> BoxResult<PluginConfig> {
        let process_header_modifier = |header_modifier: HttpRequestHeaderFilter, modifier_kind: SgFilterHeaderModifierKind| -> BoxResult<PluginConfig> {
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

            Ok(PluginConfig {
                id: PluginInstanceId {
                    code: SG_FILTER_HEADER_MODIFIER_CODE.into(),
                    name: PluginInstanceName::Mono {},
                },
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
            k8s_gateway_api::HttpRouteFilter::RequestRedirect { request_redirect } => PluginConfig {
                id: PluginInstanceId {
                    code: SG_FILTER_REDIRECT_CODE.into(),
                    name: PluginInstanceName::Mono {},
                },
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
            k8s_gateway_api::HttpRouteFilter::URLRewrite { url_rewrite } => PluginConfig {
                id: PluginInstanceId {
                    code: SG_FILTER_REWRITE_CODE.into(),
                    name: PluginInstanceName::Mono {},
                },
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

    async fn add_filter_target(&self, target: K8sSgFilterSpecTargetRef, client: &K8s) {
        todo!()
    }

    async fn to_route_filter_or_add_filter_target(&self, target: K8sSgFilterSpecTargetRef, client: &K8s) -> Option<HttpRouteFilter> {
        todo!()
    }
}

pub(crate) trait PathModifierConv {
    fn to_http_path_modifier(self) -> HttpPathModifier;
}

impl PathModifierConv for SgHttpPathModifier {
    fn to_http_path_modifier(self) -> HttpPathModifier {
        match self.kind {
            SgHttpPathModifierType::ReplaceFullPath => HttpPathModifier::ReplaceFullPath { replace_full_path: self.value },
            SgHttpPathModifierType::ReplacePrefixMatch => HttpPathModifier::ReplacePrefixMatch { replace_prefix_match: self.value },
        }
    }
}
