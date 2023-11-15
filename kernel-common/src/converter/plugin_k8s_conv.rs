use crate::constants::k8s_constants::DEFAULT_NAMESPACE;
use crate::gatewayapi_support_filter::{
    SgFilterHeaderModifier, SgFilterHeaderModifierKind, SgFilterRedirect, SgFilterRewrite, SG_FILTER_HEADER_MODIFIER_CODE, SG_FILTER_REDIRECT_CODE, SG_FILTER_REWRITE_CODE,
};
use crate::inner_model::plugin_filter::{SgHttpPathModifier, SgHttpPathModifierType, SgRouteFilter};
use crate::k8s_crd::sg_filter::{K8sSgFilterSpecFilter, K8sSgFilterSpecTargetRef};
use k8s_gateway_api::{HttpHeader, HttpPathModifier, HttpRequestHeaderFilter, HttpRequestRedirectFilter, HttpRouteFilter, HttpUrlRewriteFilter};
use std::hash::{Hash, Hasher};
use tardis::TardisFuns;

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
                    enable: true,
                    config: self.spec,
                },
                target_ref: target,
            })
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
