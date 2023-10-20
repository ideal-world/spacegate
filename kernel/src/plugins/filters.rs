pub mod compression;
pub mod header_modifier;
mod inject;
#[cfg(feature = "cache")]
mod limit;
pub mod maintenance;
pub mod redirect;
pub mod retry;
pub mod rewrite;
pub mod status;
use async_trait::async_trait;

use core::fmt;
use serde_json::Value;
use std::collections::HashMap;

use tardis::basic::error::TardisError;
use tardis::basic::result::TardisResult;
use tardis::url::Url;
use tardis::{log, TardisFuns};

use crate::instance::SgHttpRouteMatchInst;
use kernel_common::inner_model::gateway::{SgGateway, SgParameters};
use kernel_common::inner_model::http_route::{SgBackendRef, SgHttpPathMatchType, SgHttpRoute, SgHttpRouteRule};
use kernel_common::inner_model::plugin_filter::{SgHttpPathModifier, SgHttpPathModifierType, SgRouteFilter};

use super::context::SgRoutePluginContext;

static mut FILTERS: Option<HashMap<String, Box<dyn SgPluginFilterDef>>> = None;

/// # Generate filter definition
/// ## Concept Note
/// ### Filter definition
/// Filter definitions are used to register filters see[crate::register_filter_def]
///
/// ## Parameter Description
/// ### code
/// Defines a unique code for a plugins, used to specify this code in
/// the configuration to use this plug-in
/// ### filter_def
/// The recommended naming convention is `{filter_type}Def`
/// ### filter_type
/// Actual struct of Filter
#[macro_export]
macro_rules! def_filter {
    ($code:expr, $filter_def:ident, $filter_type:ty) => {
        pub const CODE: &str = $code;

        pub struct $filter_def;

        impl $crate::plugins::filters::SgPluginFilterDef for $filter_def {
            fn get_code(&self) -> &'static str {
                CODE
            }
            fn inst(&self, spec: serde_json::Value) -> TardisResult<$crate::plugins::filters::BoxSgPluginFilter> {
                let filter = tardis::TardisFuns::json.json_to_obj::<$filter_type>(spec)?;
                Ok(filter.boxed())
            }
        }
    };
}

fn init_filter_defs() {
    let mut filters: HashMap<String, Box<dyn SgPluginFilterDef>> = HashMap::new();
    filters.insert(header_modifier::CODE.to_string(), Box::new(header_modifier::SgFilterHeaderModifierDef));
    filters.insert(rewrite::CODE.to_string(), Box::new(rewrite::SgFilterRewriteDef));
    filters.insert(redirect::CODE.to_string(), Box::new(redirect::SgFilterRedirectDef));
    filters.insert(inject::CODE.to_string(), Box::new(inject::SgFilterInjectDef));
    #[cfg(feature = "cache")]
    filters.insert(limit::CODE.to_string(), Box::new(limit::SgFilterLimitDef));
    filters.insert(compression::CODE.to_string(), Box::new(compression::SgFilterCompressionDef));
    filters.insert(status::CODE.to_string(), Box::new(status::SgFilterStatusDef));
    filters.insert(maintenance::CODE.to_string(), Box::new(maintenance::SgFilterMaintenanceDef));
    filters.insert(retry::CODE.to_string(), Box::new(retry::SgFilterRetryDef));
    unsafe {
        FILTERS = Some(filters);
    }
}

pub fn register_filter_def(code: impl Into<String>, filter_def: Box<dyn SgPluginFilterDef>) {
    unsafe {
        if FILTERS.is_none() {
            init_filter_defs();
        }
        FILTERS.as_mut().expect("Unreachable code").insert(code.into(), filter_def);
    }
}

pub fn get_filter_def(code: &str) -> TardisResult<&dyn SgPluginFilterDef> {
    unsafe {
        if FILTERS.is_none() {
            init_filter_defs();
        }
        FILTERS
            .as_ref()
            .expect("Unreachable code")
            .get(code)
            .map(|f| f.as_ref())
            .ok_or_else(|| TardisError::format_error(&format!("[SG.FILTER] Filter code '{code}' not found"), ""))
    }
}

pub async fn init(filter_configs: Vec<SgRouteFilter>, init_dto: SgPluginFilterInitDto) -> TardisResult<Vec<(String, BoxSgPluginFilter)>> {
    let mut plugin_filters: Vec<(String, BoxSgPluginFilter)> = Vec::new();
    let mut elements_to_remove = vec![];
    for filter_conf in filter_configs {
        let name = filter_conf.name.unwrap_or(TardisFuns::field.nanoid());
        //todo k8s update sgfilter.name
        let filter_def = get_filter_def(&filter_conf.code)?;
        let filter_inst = filter_def.inst(filter_conf.spec)?;
        plugin_filters.push((format!("{}_{name}", filter_conf.code), filter_inst));
    }
    for (i, (id, plugin_filter)) in plugin_filters.iter_mut().enumerate() {
        log::trace!("[SG.Filter] init {id} from {} .....", init_dto.attached_level);
        if plugin_filter.init(&init_dto).await.is_err() {
            elements_to_remove.push(i);
        }
    }
    for &i in elements_to_remove.iter().rev() {
        log::info!("[SG.Filter] Remove filter: {}", plugin_filters.remove(i).0);
    }
    Ok(plugin_filters)
}

pub trait SgPluginFilterDef {
    fn get_code(&self) -> &str;
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

    async fn init(&mut self, init_dto: &SgPluginFilterInitDto) -> TardisResult<()>;

    async fn destroy(&self) -> TardisResult<()>;

    /// Request Filtering:
    ///
    /// This method is used for request filtering. It takes two parameters:
    ///
    /// - `id`: The plugin instance ID, which identifies the specific plugin
    /// instance.
    /// - `ctx`: A mutable context object that holds information about the
    /// request and allows for modifications.
    async fn req_filter(&self, id: &str, mut ctx: SgRoutePluginContext) -> TardisResult<(bool, SgRoutePluginContext)>;

    /// Response Filtering:
    ///
    /// This method is used for response filtering. It takes two parameters:
    ///
    /// - `id`: The plugin instance ID, which identifies the specific plugin
    /// instance.
    /// - `ctx`: A mutable context object that holds information about the
    /// request and allows for modifications.
    async fn resp_filter(&self, id: &str, mut ctx: SgRoutePluginContext) -> TardisResult<(bool, SgRoutePluginContext)>;

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
                log::debug!(
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
                            log::debug!(
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
                            log::debug!(
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
                    log::debug!(
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
/// The SgPluginFilterKind enum is used to represent the types of plugins
/// supported by Spacegate or to identify the type of the current request.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SgPluginFilterKind {
    Http,
    Grpc,
    Ws,
}

/// The SgAttachedLevel enum is used to represent the levels at which a plugin
/// can be attached within
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SgAttachedLevel {
    Gateway,
    HttpRoute,
    Rule,
    Backend,
}

impl fmt::Display for SgAttachedLevel {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            SgAttachedLevel::Gateway => write!(f, "GateWay"),
            SgAttachedLevel::HttpRoute => write!(f, "HttpRoute"),
            SgAttachedLevel::Rule => write!(f, "Rule"),
            SgAttachedLevel::Backend => write!(f, "Backend"),
        }
    }
}

/// Encapsulation filter initialization parameters.
#[derive(Debug, Clone)]
pub struct SgPluginFilterInitDto {
    pub gateway_name: String,
    /// Provide gateway-level public configuration
    pub gateway_parameters: SgParameters,
    pub http_route_rules: Vec<SgHttpRouteRule>,
    /// Identifies the level to which the filter is attached
    pub attached_level: SgAttachedLevel,
}

impl SgPluginFilterInitDto {
    pub fn from_global(gateway_conf: &SgGateway, routes: &[SgHttpRoute]) -> Self {
        Self {
            gateway_name: gateway_conf.name.clone(),
            gateway_parameters: gateway_conf.parameters.clone(),
            http_route_rules: routes.iter().flat_map(|route| route.rules.clone().unwrap_or_default()).collect::<Vec<_>>(),
            attached_level: SgAttachedLevel::Gateway,
        }
    }
    pub fn from_route(gateway_conf: &SgGateway, route: &SgHttpRoute) -> Self {
        Self {
            gateway_name: gateway_conf.name.clone(),
            gateway_parameters: gateway_conf.parameters.clone(),
            http_route_rules: route.rules.clone().unwrap_or_default(),
            attached_level: SgAttachedLevel::HttpRoute,
        }
    }
    pub fn from_rule(gateway_conf: &SgGateway, rule: &SgHttpRouteRule) -> Self {
        Self {
            gateway_name: gateway_conf.name.clone(),
            gateway_parameters: gateway_conf.parameters.clone(),
            http_route_rules: vec![rule.clone()],
            attached_level: SgAttachedLevel::Rule,
        }
    }

    pub fn from_backend(gateway_conf: &SgGateway, rule: &SgHttpRouteRule, backend: &SgBackendRef) -> Self {
        let mut rule = rule.clone();
        rule.backends = Some(vec![backend.clone()]);
        Self {
            gateway_name: gateway_conf.name.clone(),
            gateway_parameters: gateway_conf.parameters.clone(),
            http_route_rules: vec![rule],
            attached_level: SgAttachedLevel::Backend,
        }
    }
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

mod tests {
    use kernel_common::inner_model::http_route::SgHttpPathMatchType;
    use kernel_common::inner_model::plugin_filter::{SgHttpPathModifier, SgHttpPathModifierType};
    use tardis::{basic::result::TardisResult, regex::Regex};

    use crate::{
        instance::{SgHttpPathMatchInst, SgHttpRouteMatchInst},
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
