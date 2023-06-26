pub mod compression;
pub mod header_modifier;
mod inject;
#[cfg(feature = "cache")]
mod limit;
pub mod maintenance;
pub mod redirect;
pub mod retry;
pub mod rewrite;
#[cfg(feature = "web")]
pub mod status;
use async_trait::async_trait;

use serde_json::Value;
use std::collections::HashMap;

use tardis::basic::error::TardisError;
use tardis::basic::result::TardisResult;
use tardis::url::Url;
use tardis::{log, TardisFuns};

use crate::config::http_route_dto::{SgHttpPathMatchType, SgHttpRouteRule};
use crate::config::plugin_filter_dto::{SgHttpPathModifier, SgHttpPathModifierType, SgRouteFilter};
use crate::functions::http_route::SgHttpRouteMatchInst;

use super::context::SgRoutePluginContext;

static mut FILTERS: Option<HashMap<String, Box<dyn SgPluginFilterDef>>> = None;

fn init_filter_defs() {
    let mut filters: HashMap<String, Box<dyn SgPluginFilterDef>> = HashMap::new();
    filters.insert(header_modifier::CODE.to_string(), Box::new(header_modifier::SgFilterHeaderModifierDef));
    filters.insert(rewrite::CODE.to_string(), Box::new(rewrite::SgFilterRewriteDef));
    filters.insert(redirect::CODE.to_string(), Box::new(redirect::SgFilterRedirectDef));
    filters.insert(inject::CODE.to_string(), Box::new(inject::SgFilterInjectDef));
    #[cfg(feature = "cache")]
    filters.insert(limit::CODE.to_string(), Box::new(limit::SgFilterLimitDef));
    filters.insert(compression::CODE.to_string(), Box::new(compression::SgFilterCompressionDef));
    unsafe {
        FILTERS = Some(filters);
    }
}

pub fn register_filter_def(code: &str, filter_def: Box<dyn SgPluginFilterDef>) {
    unsafe {
        if FILTERS.is_none() {
            init_filter_defs();
        }
        FILTERS.as_mut().expect("Unreachable code").insert(code.to_string(), filter_def);
    }
}

pub fn get_filter_def(code: &str) -> TardisResult<&Box<dyn SgPluginFilterDef>> {
    unsafe {
        if FILTERS.is_none() {
            init_filter_defs();
        }
        FILTERS.as_ref().expect("Unreachable code").get(code).ok_or_else(|| TardisError::format_error(&format!("[SG.FILTER] Filter code '{code}' not found"), ""))
    }
}

pub async fn init(filter_configs: Vec<SgRouteFilter>, http_route_rules: &[SgHttpRouteRule]) -> TardisResult<Vec<(String, BoxSgPluginFilter)>> {
    let mut plugin_filters: Vec<(String, BoxSgPluginFilter)> = Vec::new();
    for filter_conf in filter_configs {
        let name = filter_conf.name.unwrap_or(TardisFuns::field.nanoid());
        let filter_def = get_filter_def(&filter_conf.code)?;
        let filter_inst = filter_def.inst(filter_conf.spec)?;
        plugin_filters.push((format!("{}_{name}", filter_conf.code), filter_inst));
    }
    for (_, plugin_filter) in &plugin_filters {
        plugin_filter.init(http_route_rules).await?;
    }
    Ok(plugin_filters)
}

pub trait SgPluginFilterDef {
    fn inst(&self, spec: Value) -> TardisResult<BoxSgPluginFilter>;
}

pub type BoxSgPluginFilter = Box<dyn SgPluginFilter>;

#[async_trait]
pub trait SgPluginFilter: Send + Sync + 'static {
    /// Enable the filter to have a state that determines
    /// whether to execute the filter at runtime
    fn accept(&self) -> SgPluginFilterAccept {
        SgPluginFilterAccept::default()
    }

    /// Whether to filter the response
    fn before_resp_filter_check(&self, ctx: &SgRoutePluginContext) -> bool {
        let accept_error_response = if ctx.is_resp_error() { self.accept().accept_error_response } else { true };
        if accept_error_response {
            self.accept().kind.contains(ctx.get_request_kind())
        } else {
            false
        }
    }

    async fn init(&self, http_route_rule: &[SgHttpRouteRule]) -> TardisResult<()>;

    async fn destroy(&self) -> TardisResult<()>;

    async fn req_filter(&self, id: &str, mut ctx: SgRoutePluginContext, matched_match_inst: Option<&SgHttpRouteMatchInst>) -> TardisResult<(bool, SgRoutePluginContext)>;

    async fn resp_filter(&self, id: &str, mut ctx: SgRoutePluginContext, matched_match_inst: Option<&SgHttpRouteMatchInst>) -> TardisResult<(bool, SgRoutePluginContext)>;

    fn boxed(self) -> BoxSgPluginFilter
    where
        Self: Sized,
    {
        Box::new(self)
    }
}

pub fn http_common_modify_path(uri: &http::Uri, modify_path: &Option<SgHttpPathModifier>, matched_match_inst: Option<&SgHttpRouteMatchInst>) -> TardisResult<Option<http::Uri>> {
    if let Some(modify_path) = &modify_path {
        let mut uri = Url::parse(&uri.to_string())?;
        match modify_path.kind {
            SgHttpPathModifierType::ReplaceFullPath => {
                log::trace!(
                    "[SG.Plugin.Filter.Common] Modify path with modify kind [ReplaceFullPath], form {} to  {}",
                    uri.path(),
                    modify_path.value
                );
                uri.set_path(&modify_path.value);
            }
            SgHttpPathModifierType::ReplacePrefixMatch => {
                if let Some(Some(matched_path)) = matched_match_inst.map(|m| m.path.as_ref()) {
                    match matched_path.kind {
                        SgHttpPathMatchType::Exact => {
                            // equivalent to ` SgHttpPathModifierType::ReplaceFullPath`
                            // https://cloud.yandex.com/en/docs/application-load-balancer/k8s-ref/http-route
                            log::trace!(
                                "[SG.Plugin.Filter.Common] Modify path with modify kind [ReplacePrefixMatch] and match kind [Exact], form {} to {}",
                                uri.path(),
                                modify_path.value
                            );
                            uri.set_path(&modify_path.value);
                        }
                        _ => {
                            let origin_path = uri.path();
                            let match_path = if matched_path.kind == SgHttpPathMatchType::Prefix {
                                &matched_path.value
                            } else {
                                // Support only one capture group
                                matched_path.regular.as_ref().expect("").captures(origin_path).map(|cap| cap.get(1).map_or("", |m| m.as_str())).unwrap_or("")
                            };
                            let match_path_reduce = origin_path.strip_prefix(match_path).ok_or_else(|| {
                                TardisError::format_error(
                                    "[SG.Plugin.Filter.Common] Modify path with modify kind [ReplacePrefixMatch] and match kind [Exact] failed",
                                    "",
                                )
                            })?;
                            let new_path = if match_path_reduce.is_empty() {
                                modify_path.value.to_string()
                            } else if match_path_reduce.starts_with('/') && modify_path.value.ends_with('/') {
                                format!("{}{}", modify_path.value, &match_path_reduce.to_string()[1..])
                            } else if match_path_reduce.starts_with('/') || modify_path.value.ends_with('/') {
                                format!("{}{}", modify_path.value, &match_path_reduce.to_string())
                            } else {
                                format!("{}/{}", modify_path.value, &match_path_reduce.to_string())
                            };
                            log::trace!(
                                "[SG.Plugin.Filter.Common] Modify path with modify kind [ReplacePrefixMatch] and match kind [Prefix/Regular], form {} to {}",
                                origin_path,
                                new_path,
                            );
                            uri.set_path(&new_path);
                        }
                    }
                } else {
                    // TODO
                    // equivalent to ` SgHttpPathModifierType::ReplaceFullPath`
                    log::trace!(
                        "[SG.Plugin.Filter.Common] Modify path with modify kind [None], form {} to {}",
                        uri.path(),
                        modify_path.value,
                    );
                    uri.set_path(&modify_path.value);
                }
            }
        }
        return Ok(Some(
            uri.as_str().parse().map_err(|e| TardisError::internal_error(&format!("[SG.Plugin.Filter.Common] uri parse error: {}", e), ""))?,
        ));
    }
    Ok(None)
}

// TODO
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SgPluginFilterKind {
    Http,
    Grpc,
    Ws,
}
#[derive(Debug, Clone)]
pub struct SgPluginFilterAccept {
    pub kind: Vec<SgPluginFilterKind>,
    /// Whether to accept the error response, default is false .
    ///
    /// if filter can accept the error response, it should return true
    pub accept_error_response: bool,
}

impl Default for SgPluginFilterAccept {
    fn default() -> Self {
        Self {
            kind: vec![SgPluginFilterKind::Http],
            accept_error_response: false,
        }
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use tardis::{basic::result::TardisResult, regex::Regex};

    use crate::{
        config::{
            http_route_dto::SgHttpPathMatchType,
            plugin_filter_dto::{SgHttpPathModifier, SgHttpPathModifierType},
        },
        functions::http_route::{SgHttpPathMatchInst, SgHttpRouteMatchInst},
        plugins::filters::http_common_modify_path,
    };

    #[test]
    fn test_http_common_modify_path() -> TardisResult<()> {
        let url = "http://sg.idealworld.group/iam/ct/001?name=sg".parse().unwrap();

        let path_prefix_modifier = SgHttpPathModifier {
            kind: SgHttpPathModifierType::ReplacePrefixMatch,
            value: "/new_iam".to_string(),
        };

        let path_full_modifier = SgHttpPathModifier {
            kind: SgHttpPathModifierType::ReplaceFullPath,
            value: "/other_iam".to_string(),
        };

        // with nothing
        assert!(http_common_modify_path(&url, &None, None)?.is_none());

        // without match inst
        assert_eq!(
            http_common_modify_path(&url, &Some(path_prefix_modifier.clone()), None)?.unwrap().to_string(),
            "http://sg.idealworld.group/new_iam?name=sg".to_string()
        );
        assert_eq!(
            http_common_modify_path(&url, &Some(path_full_modifier), None)?.unwrap().to_string(),
            "http://sg.idealworld.group/other_iam?name=sg".to_string()
        );

        // with math inst
        let exact_match_inst = SgHttpRouteMatchInst {
            path: Some(SgHttpPathMatchInst {
                kind: SgHttpPathMatchType::Exact,
                value: "/iam".to_string(),
                regular: None,
            }),
            ..Default::default()
        };
        let prefix_match_inst = SgHttpRouteMatchInst {
            path: Some(SgHttpPathMatchInst {
                kind: SgHttpPathMatchType::Prefix,
                value: "/iam".to_string(),
                regular: None,
            }),
            ..Default::default()
        };
        let regular_match_inst = SgHttpRouteMatchInst {
            path: Some(SgHttpPathMatchInst {
                kind: SgHttpPathMatchType::Regular,
                value: "(/[a-z]+)".to_string(),
                regular: Some(Regex::new("(/[a-z]+)")?),
            }),
            ..Default::default()
        };
        assert_eq!(
            http_common_modify_path(&url, &Some(path_prefix_modifier.clone()), Some(&exact_match_inst))?.unwrap().to_string(),
            "http://sg.idealworld.group/new_iam?name=sg".to_string()
        );
        assert_eq!(
            http_common_modify_path(&url, &Some(path_prefix_modifier.clone()), Some(&prefix_match_inst))?.unwrap().to_string(),
            "http://sg.idealworld.group/new_iam/ct/001?name=sg".to_string()
        );
        assert_eq!(
            http_common_modify_path(&url, &Some(path_prefix_modifier), Some(&regular_match_inst))?.unwrap().to_string(),
            "http://sg.idealworld.group/new_iam/ct/001?name=sg".to_string()
        );

        Ok(())
    }
}
