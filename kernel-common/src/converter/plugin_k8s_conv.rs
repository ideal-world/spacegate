use crate::client::k8s_client;
use crate::constants::k8s_constants::DEFAULT_NAMESPACE;
use crate::gatewayapi_support_filter::{
    SgFilterHeaderModifier, SgFilterHeaderModifierKind, SgFilterRedirect, SgFilterRewrite, SG_FILTER_HEADER_MODIFIER_CODE, SG_FILTER_REDIRECT_CODE, SG_FILTER_REWRITE_CODE,
};
use crate::inner_model::plugin_filter::{SgHttpPathModifier, SgHttpPathModifierType, SgRouteFilter};
use crate::k8s_crd::sg_filter::{K8sSgFilterSpecFilter, K8sSgFilterSpecTargetRef, SgFilter};
use k8s_gateway_api::{HttpHeader, HttpPathModifier, HttpRequestHeaderFilter, HttpRequestRedirectFilter, HttpRouteFilter, HttpUrlRewriteFilter};
use kube::api::ListParams;
use kube::Api;
use std::collections::HashMap;
use std::hash::{Hash, Hasher};
use tardis::basic::error::TardisError;
use tardis::basic::result::TardisResult;
use tardis::{log, TardisFuns};

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
                namespace: target.namespace.clone().unwrap_or(DEFAULT_NAMESPACE.to_string()),
                filter: K8sSgFilterSpecFilter {
                    code: self.code,
                    name: None,
                    enable: self.enable,
                    config: self.spec,
                },
                target_ref: target,
            })
        }
    }

    pub(crate) async fn from_crd_filters(client_name: &str, kind: &str, name: &Option<String>, namespace: &Option<String>) -> TardisResult<Option<Vec<SgRouteFilter>>> {
        let name = name.clone().ok_or_else(|| TardisError::format_error(&format!("[SG.Common] {kind} [metadata.name] is required"), ""))?;
        let namespace = namespace.clone().unwrap_or("default".to_string());

        let filter_api: Api<SgFilter> = Api::all((*k8s_client::get(client_name).await?).clone());
        let filter_objs: Vec<SgRouteFilter> = filter_api
            .list(&ListParams::default())
            .await
            .map_err(|error| TardisError::wrap(&format!("[SG.Config] Kubernetes error: {error:?}"), ""))?
            .into_iter()
            .filter(|filter_obj| {
                filter_obj.spec.target_refs.iter().any(|target_ref| {
                    target_ref.kind.eq_ignore_ascii_case(kind)
                        && target_ref.name.eq_ignore_ascii_case(&name)
                        && target_ref.namespace.as_deref().unwrap_or("default").eq_ignore_ascii_case(&namespace)
                })
            })
            .flat_map(|filter_obj| {
                filter_obj.spec.filters.into_iter().map(|filter| SgRouteFilter {
                    code: filter.code,
                    name: filter.name,
                    spec: filter.config,
                    enable: filter.enable,
                })
            })
            .collect();

        if !filter_objs.is_empty() {
            let mut filter_vec = String::new();
            filter_objs.clone().into_iter().for_each(|filter| filter_vec.push_str(&format!("Filter{{code: {},name:{}}},", filter.code, filter.name.unwrap_or("None".to_string()))));
            log::trace!("[SG.Common] {namespace}.{kind}.{name} filter found: {}", filter_vec.trim_end_matches(','));
        }

        if filter_objs.is_empty() {
            Ok(None)
        } else {
            Ok(Some(filter_objs))
        }
    }

    /// # to_http_route_filter
    /// ref [SgRouteFilter::to_singe_filter]
    pub fn to_http_route_filter(self) -> Option<HttpRouteFilter> {
        if &self.code == SG_FILTER_HEADER_MODIFIER_CODE {
            if let Ok(header) = TardisFuns::json.json_to_obj::<SgFilterHeaderModifier>(self.spec) {
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
        } else if &self.code == SG_FILTER_REDIRECT_CODE {
            if let Ok(redirect) = TardisFuns::json.json_to_obj::<SgFilterRedirect>(self.spec) {
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
        } else if &self.code == SG_FILTER_REWRITE_CODE {
            if let Ok(rewrite) = TardisFuns::json.json_to_obj::<SgFilterRewrite>(self.spec) {
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

    pub fn from_http_route_filter(route_filter: HttpRouteFilter) -> TardisResult<SgRouteFilter> {
        let sg_filter = match route_filter {
            k8s_gateway_api::HttpRouteFilter::RequestHeaderModifier { request_header_modifier } => {
                let mut sg_sets = HashMap::new();
                if let Some(adds) = request_header_modifier.add {
                    for add in adds {
                        sg_sets.insert(add.name, add.value);
                    }
                }
                if let Some(sets) = request_header_modifier.set {
                    for set in sets {
                        sg_sets.insert(set.name, set.value);
                    }
                }
                SgRouteFilter {
                    code: SG_FILTER_HEADER_MODIFIER_CODE.to_string(),
                    name: None,
                    spec: TardisFuns::json.obj_to_json(&SgFilterHeaderModifier {
                        kind: SgFilterHeaderModifierKind::Request,
                        sets: if sg_sets.is_empty() { None } else { Some(sg_sets) },
                        remove: request_header_modifier.remove,
                    })?,
                    enable: true,
                }
            }
            k8s_gateway_api::HttpRouteFilter::ResponseHeaderModifier { response_header_modifier } => {
                let mut sg_sets = HashMap::new();
                if let Some(adds) = response_header_modifier.add {
                    for add in adds {
                        sg_sets.insert(add.name, add.value);
                    }
                }
                if let Some(sets) = response_header_modifier.set {
                    for set in sets {
                        sg_sets.insert(set.name, set.value);
                    }
                }
                SgRouteFilter {
                    code: SG_FILTER_HEADER_MODIFIER_CODE.to_string(),
                    name: None,
                    spec: TardisFuns::json.obj_to_json(&SgFilterHeaderModifier {
                        kind: SgFilterHeaderModifierKind::Response,
                        sets: if sg_sets.is_empty() { None } else { Some(sg_sets) },
                        remove: response_header_modifier.remove,
                    })?,
                    enable: true,
                }
            }
            k8s_gateway_api::HttpRouteFilter::RequestRedirect { request_redirect } => SgRouteFilter {
                code: SG_FILTER_REDIRECT_CODE.to_string(),
                name: None,
                enable: true,
                spec: TardisFuns::json.obj_to_json(&SgFilterRedirect {
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
                enable: true,
                spec: TardisFuns::json.obj_to_json(&SgFilterRewrite {
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
            k8s_gateway_api::HttpRouteFilter::RequestMirror { .. } => {
                return Err(TardisError::not_implemented(
                    "[SG.Common] HttpRoute [spec.rules.filters.type=RequestMirror] not supported yet",
                    "",
                ))
            }
            k8s_gateway_api::HttpRouteFilter::ExtensionRef { .. } => {
                return Err(TardisError::not_implemented(
                    "[SG.Common] HttpRoute [spec.rules.filters.type=ExtensionRef] not supported yet",
                    "",
                ))
            }
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

#[cfg(feature = "k8s")]
#[derive(Clone)]
pub struct SgSingeFilter {
    pub name: Option<String>,
    pub namespace: String,
    pub filter: crate::k8s_crd::sg_filter::K8sSgFilterSpecFilter,
    pub target_ref: crate::k8s_crd::sg_filter::K8sSgFilterSpecTargetRef,
}

impl Hash for SgSingeFilter {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.namespace.hash(state);
        self.filter.code.hash(state);
        self.target_ref.kind.hash(state);
        self.target_ref.name.hash(state);
        self.target_ref.namespace.hash(state);
    }
}

impl PartialEq<Self> for SgSingeFilter {
    fn eq(&self, other: &Self) -> bool {
        self.namespace == other.namespace
            && self.filter.code == other.filter.code
            && self.target_ref.kind == other.target_ref.kind
            && self.target_ref.name == other.target_ref.name
            && self.target_ref.namespace.as_ref().unwrap_or(&DEFAULT_NAMESPACE.to_string()) == other.target_ref.namespace.as_ref().unwrap_or(&DEFAULT_NAMESPACE.to_string())
    }
}

impl Eq for SgSingeFilter {}

#[cfg(feature = "k8s")]
impl SgSingeFilter {
    pub fn to_sg_filter(&self) -> crate::k8s_crd::sg_filter::SgFilter {
        crate::k8s_crd::sg_filter::SgFilter {
            metadata: k8s_openapi::apimachinery::pkg::apis::meta::v1::ObjectMeta {
                name: self.name.clone(),
                namespace: Some(self.namespace.clone()),
                ..Default::default()
            },
            spec: crate::k8s_crd::sg_filter::K8sSgFilterSpec {
                filters: vec![self.filter.clone()],
                target_refs: vec![self.target_ref.clone()],
            },
        }
    }
}
