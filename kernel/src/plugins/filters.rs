pub mod compression;
pub mod header_modifier;
mod inject;
#[cfg(feature = "cache")]
mod limit;
pub mod maintenance;
pub mod redirect;
pub mod rewrite;
#[cfg(feature = "web")]
pub mod status;
use async_trait::async_trait;
use http::{HeaderMap, HeaderName, HeaderValue, Method, Response, StatusCode, Uri, Version};
use hyper::Body;
use serde_json::Value;
use std::collections::HashMap;
use std::net::SocketAddr;
use tardis::basic::error::TardisError;
use tardis::basic::result::TardisResult;
use tardis::url::Url;
use tardis::{log, TardisFuns};

use crate::config::http_route_dto::{SgHttpPathMatchType, SgHttpRouteRule};
use crate::config::plugin_filter_dto::{SgHttpPathModifier, SgHttpPathModifierType, SgRouteFilter};
use crate::functions::http_route::{SgBackend, SgHttpRouteMatchInst};

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
    fn kind(&self) -> SgPluginFilterKind;

    /// Whether to filter the response
    fn before_resp_filter_check(&self, ctx: &SgRouteFilterContext) -> bool {
        if ctx.is_resp_error() {
            self.accept_error_response()
        } else {
            true
        }
    }

    /// Whether to accept the error response, default is false .
    ///
    /// if filter can accept the error response, it should return true
    fn accept_error_response(&self) -> bool {
        false
    }

    async fn init(&self, http_route_rule: &[SgHttpRouteRule]) -> TardisResult<()>;

    async fn destroy(&self) -> TardisResult<()>;

    async fn req_filter(&self, id: &str, mut ctx: SgRouteFilterContext, matched_match_inst: Option<&SgHttpRouteMatchInst>) -> TardisResult<(bool, SgRouteFilterContext)>;

    async fn resp_filter(&self, id: &str, mut ctx: SgRouteFilterContext, matched_match_inst: Option<&SgHttpRouteMatchInst>) -> TardisResult<(bool, SgRouteFilterContext)>;

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
#[allow(dead_code)]
#[derive(Debug, Clone)]
pub enum SgPluginFilterKind {
    Http,
    Grpc,
    Ws,
}

#[allow(dead_code)]
#[derive(Debug)]
pub struct SgRouteFilterContext {
    raw_req_method: Method,
    raw_req_uri: Uri,
    raw_req_version: Version,
    raw_req_body: Option<Body>,
    raw_req_headers: HeaderMap<HeaderValue>,
    raw_req_remote_addr: SocketAddr,

    mod_req_method: Option<Method>,
    mod_req_uri: Option<Uri>,
    mod_req_version: Option<Version>,
    mod_req_body: Option<Vec<u8>>,
    mod_req_headers: Option<HeaderMap<HeaderValue>>,

    raw_resp_status_code: StatusCode,
    raw_resp_headers: HeaderMap<HeaderValue>,
    raw_resp_body: Option<Body>,
    raw_resp_err: Option<TardisError>,
    mod_resp_status_code: Option<StatusCode>,
    mod_resp_headers: Option<HeaderMap<HeaderValue>>,
    mod_resp_body: Option<Vec<u8>>,

    choose_backend_name_or_host: Option<String>,

    ext: HashMap<String, String>,
    action: SgRouteFilterRequestAction,
    gateway_name: String,
}

#[derive(Debug, PartialEq, Eq, Clone)]
pub enum SgRouteFilterRequestAction {
    None,
    Redirect,
    Response,
}

#[allow(dead_code)]
impl SgRouteFilterContext {
    pub fn new(method: Method, uri: Uri, version: Version, headers: HeaderMap<HeaderValue>, body: Body, remote_addr: SocketAddr, gateway_name: String) -> Self {
        Self {
            raw_req_method: method,
            raw_req_uri: uri,
            raw_req_version: version,
            raw_req_body: Some(body),
            raw_req_headers: headers,
            raw_req_remote_addr: remote_addr,
            mod_req_method: None,
            mod_req_uri: None,
            mod_req_version: None,
            mod_req_body: None,
            mod_req_headers: None,
            raw_resp_status_code: StatusCode::OK,
            raw_resp_headers: HeaderMap::new(),
            raw_resp_body: None,
            raw_resp_err: None,
            mod_resp_status_code: None,
            mod_resp_headers: None,
            mod_resp_body: None,
            ext: HashMap::new(),
            action: SgRouteFilterRequestAction::None,
            gateway_name,
            choose_backend_name_or_host: None,
        }
    }

    ///The following two methods can only be used to fill in the context [resp] [resp_from_error]
    pub fn resp(mut self, backend: Option<&SgBackend>, status_code: StatusCode, headers: HeaderMap<HeaderValue>, body: Body) -> Self {
        self.raw_resp_status_code = status_code;
        self.raw_resp_headers = headers;
        self.raw_resp_body = Some(body);
        self.choose_backend_name_or_host = backend.map(|b| b.name_or_host.clone());
        self.raw_resp_err = None;
        self
    }

    pub fn resp_from_error(mut self, backend: Option<&SgBackend>, error: TardisError) -> Self {
        self.raw_resp_err = Some(error);
        self.raw_resp_status_code = StatusCode::INTERNAL_SERVER_ERROR;
        self.choose_backend_name_or_host = backend.map(|b| b.name_or_host.clone());
        self
    }

    pub fn is_resp_error(&self) -> bool {
        self.raw_resp_err.is_some()
    }

    pub fn get_backend_name(&self) -> Option<String> {
        self.choose_backend_name_or_host.clone()
    }

    pub fn get_req_method(&mut self) -> &Method {
        if self.mod_req_method.is_none() {
            self.mod_req_method = Some(self.raw_req_method.clone());
        }
        self.mod_req_method.as_ref().expect("Unreachable code")
    }

    pub fn set_req_method(&mut self, method: Method) {
        self.mod_req_method = Some(method);
    }

    pub fn get_req_method_raw(&self) -> &Method {
        &self.raw_req_method
    }

    pub fn get_req_uri(&mut self) -> &Uri {
        if self.mod_req_uri.is_none() {
            self.mod_req_uri = Some(self.raw_req_uri.clone());
        }
        self.mod_req_uri.as_ref().expect("Unreachable code")
    }

    pub fn set_req_uri(&mut self, uri: Uri) {
        self.mod_req_uri = Some(uri);
    }

    pub fn get_req_uri_raw(&self) -> &Uri {
        &self.raw_req_uri
    }

    pub fn get_req_version(&mut self) -> &Version {
        if self.mod_req_version.is_none() {
            self.mod_req_version = Some(self.raw_req_version);
        }
        self.mod_req_version.as_ref().expect("Unreachable code")
    }

    pub fn set_req_version(&mut self, version: Version) {
        self.mod_req_version = Some(version);
    }

    pub fn get_req_version_raw(&self) -> &Version {
        &self.raw_req_version
    }

    pub fn get_req_headers(&mut self) -> &HeaderMap<HeaderValue> {
        if self.mod_req_headers.is_none() {
            self.mod_req_headers = Some(self.raw_req_headers.clone());
        }
        self.mod_req_headers.as_ref().expect("Unreachable code")
    }

    pub fn set_req_headers(&mut self, req_headers: HeaderMap<HeaderValue>) {
        self.mod_req_headers = Some(req_headers);
    }

    pub fn set_req_header(&mut self, key: &str, value: &str) -> TardisResult<()> {
        if self.mod_req_headers.is_none() {
            self.mod_req_headers = Some(self.raw_req_headers.clone());
        }
        let mod_req_headers = self.mod_req_headers.as_mut().expect("Unreachable code");
        mod_req_headers.insert(
            HeaderName::try_from(key).map_err(|error| TardisError::format_error(&format!("[SG.Filter] Header key {key} parsing error: {error}"), ""))?,
            HeaderValue::try_from(value).map_err(|error| TardisError::format_error(&format!("[SG.Filter] Header value {value} parsing error: {error}"), ""))?,
        );
        Ok(())
    }

    pub fn remove_req_header(&mut self, key: &str) -> TardisResult<()> {
        if let Some(headers) = self.mod_req_headers.as_mut() {
            headers.remove(HeaderName::try_from(key).map_err(|error| TardisError::format_error(&format!("[SG.Filter] Header key {key} parsing error: {error}"), ""))?);
        }
        Ok(())
    }

    pub fn get_req_headers_raw(&self) -> &HeaderMap<HeaderValue> {
        &self.raw_req_headers
    }

    pub async fn pop_req_body(&mut self) -> TardisResult<Option<Vec<u8>>> {
        if self.mod_req_body.is_some() {
            let mut body = None;
            std::mem::swap(&mut body, &mut self.mod_req_body);
            Ok(body)
        } else if self.raw_req_body.is_some() {
            let mut body = None;
            std::mem::swap(&mut body, &mut self.raw_req_body);
            let body = hyper::body::to_bytes(body.expect("Unreachable code"))
                .await
                .map_err(|error| TardisError::format_error(&format!("[SG.Filter] Request Body parsing error:{error}"), ""))?;
            let body = body.iter().cloned().collect::<Vec<u8>>();
            Ok(Some(body))
        } else {
            Ok(None)
        }
    }

    pub fn set_req_body(&mut self, body: Vec<u8>) -> TardisResult<()> {
        self.set_req_header("Content-Length", body.len().to_string().as_str())?;
        self.mod_req_body = Some(body);
        Ok(())
    }

    pub fn pop_req_body_raw(&mut self) -> TardisResult<Option<Body>> {
        if self.mod_req_body.is_some() {
            let mut body = None;
            std::mem::swap(&mut body, &mut self.mod_req_body);
            Ok(body.map(Body::from))
        } else if self.raw_req_body.is_some() {
            let mut body = None;
            std::mem::swap(&mut body, &mut self.raw_req_body);
            Ok(body)
        } else {
            Ok(None)
        }
    }

    pub fn get_req_remote_addr(&self) -> &SocketAddr {
        &self.raw_req_remote_addr
    }

    pub fn get_resp_status_code(&mut self) -> &StatusCode {
        if self.mod_resp_status_code.is_none() {
            self.mod_resp_status_code = Some(self.raw_resp_status_code);
        }
        self.mod_resp_status_code.as_ref().expect("Unreachable code")
    }

    pub fn set_resp_status_code(&mut self, status_code: StatusCode) {
        self.mod_resp_status_code = Some(status_code);
    }

    pub fn get_resp_status_code_raw(&self) -> &StatusCode {
        &self.raw_resp_status_code
    }

    pub fn get_resp_headers(&mut self) -> &HeaderMap<HeaderValue> {
        if self.mod_resp_headers.is_none() {
            self.mod_resp_headers = Some(self.raw_resp_headers.clone());
        }
        self.mod_resp_headers.as_ref().expect("Unreachable code")
    }

    pub fn set_resp_headers(&mut self, resp_headers: HeaderMap<HeaderValue>) {
        self.mod_resp_headers = Some(resp_headers);
    }

    pub fn set_resp_header(&mut self, key: &str, value: &str) -> TardisResult<()> {
        if self.mod_resp_headers.is_none() {
            self.mod_resp_headers = Some(self.raw_resp_headers.clone());
        }
        let mod_resp_headers = self.mod_resp_headers.as_mut().expect("Unreachable code");
        mod_resp_headers.insert(
            HeaderName::try_from(key).map_err(|error| TardisError::format_error(&format!("[SG.Filter] Header key {key} parsing error: {error}"), ""))?,
            HeaderValue::try_from(value).map_err(|error| TardisError::format_error(&format!("[SG.Filter] Header value {value} parsing error: {error}"), ""))?,
        );
        Ok(())
    }

    pub fn remove_resp_header(&mut self, key: &str) -> TardisResult<()> {
        if let Some(headers) = self.mod_resp_headers.as_mut() {
            headers.remove(HeaderName::try_from(key).map_err(|error| TardisError::format_error(&format!("[SG.Filter] Header key {key} parsing error: {error}"), ""))?);
        }
        Ok(())
    }

    pub fn get_resp_headers_raw(&self) -> &HeaderMap<HeaderValue> {
        &self.raw_resp_headers
    }

    pub async fn pop_resp_body(&mut self) -> TardisResult<Option<Vec<u8>>> {
        if self.mod_resp_body.is_some() {
            let mut body = None;
            std::mem::swap(&mut body, &mut self.mod_resp_body);
            Ok(body)
        } else if self.raw_resp_body.is_some() {
            let mut body = None;
            std::mem::swap(&mut body, &mut self.raw_resp_body);
            let body = hyper::body::to_bytes(body.expect("Unreachable code"))
                .await
                .map_err(|error| TardisError::format_error(&format!("[SG.Filter] Response Body parsing error:{error}"), ""))?;
            let body = body.iter().cloned().collect::<Vec<u8>>();
            Ok(Some(body))
        } else {
            Ok(None)
        }
    }
    /// build response from Context
    pub async fn build_response(&mut self) -> TardisResult<Response<Body>> {
        if let Some(err) = &self.raw_resp_err {
            return Err(err.clone());
        }
        let mut resp = Response::builder();
        for (k, v) in self.get_resp_headers() {
            resp = resp.header(
                k.as_str(),
                v.to_str().map_err(|_| TardisError::bad_request(&format!("[SG.Route] header {k}'s value illegal: is not ascii"), ""))?.to_string(),
            );
        }
        let resp = resp
            .body(Body::from(self.pop_resp_body().await?.unwrap_or_default()))
            .map_err(|error| TardisError::internal_error(&format!("[SG.Route] Build response error:{error}"), ""))?;
        Ok(resp)
    }

    pub fn set_resp_body(&mut self, body: Vec<u8>) -> TardisResult<()> {
        self.set_resp_header("Content-Length", body.len().to_string().as_str())?;
        self.mod_resp_body = Some(body);
        Ok(())
    }

    pub fn pop_resp_body_raw(&mut self) -> TardisResult<Option<Body>> {
        if self.mod_resp_body.is_some() {
            let mut body = None;
            std::mem::swap(&mut body, &mut self.mod_resp_body);
            Ok(body.map(Body::from))
        } else if self.raw_resp_body.is_some() {
            let mut body = None;
            std::mem::swap(&mut body, &mut self.raw_resp_body);
            Ok(body)
        } else {
            Ok(None)
        }
    }

    pub fn get_ext(&self, key: &str) -> Option<String> {
        self.ext.get(key).map(|value| value.to_string())
    }

    pub fn set_ext(&mut self, key: &str, value: &str) {
        self.ext.insert(key.to_string(), value.to_string());
    }

    pub fn remove_ext(&mut self, key: &str) {
        self.ext.remove(key);
    }

    pub fn get_action(&self) -> &SgRouteFilterRequestAction {
        &self.action
    }

    pub fn set_action(&mut self, action: SgRouteFilterRequestAction) {
        self.action = action;
    }

    #[cfg(feature = "cache")]
    pub fn cache(&self) -> TardisResult<&'static tardis::cache::cache_client::TardisCacheClient> {
        crate::functions::cache_client::get(&self.gateway_name)
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
