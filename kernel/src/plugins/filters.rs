pub mod header_modifier;
pub mod redirect;
use async_trait::async_trait;
use http::{HeaderMap, HeaderName, HeaderValue, Method, StatusCode, Uri, Version};
use hyper::Body;
use serde_json::Value;
use std::collections::HashMap;
use std::net::SocketAddr;
use tardis::basic::error::TardisError;
use tardis::basic::result::TardisResult;
use tardis::TardisFuns;

use crate::config::plugin_filter_dto::SgRouteFilter;

static mut FILTERS: Option<HashMap<String, Box<dyn SgPluginFilterDef>>> = None;

fn init_filter_defs() {
    let mut filters: HashMap<String, Box<dyn SgPluginFilterDef>> = HashMap::new();
    filters.insert(header_modifier::CODE.to_string(), Box::new(header_modifier::SgFilerHeaderModifierDef));
    filters.insert(redirect::CODE.to_string(), Box::new(redirect::SgFilerRedirectDef));
    unsafe {
        FILTERS = Some(filters);
    }
}

pub fn register_filter_def(code: &str, filter_def: Box<dyn SgPluginFilterDef>) {
    unsafe {
        if FILTERS.is_none() {
            init_filter_defs();
        }
        FILTERS.as_mut().unwrap().insert(code.to_string(), filter_def);
    }
}

pub fn get_filter_def(code: &str) -> &Box<dyn SgPluginFilterDef> {
    unsafe {
        if FILTERS.is_none() {
            init_filter_defs();
        }
        FILTERS.as_ref().unwrap().get(code).unwrap()
    }
}

pub async fn init(filter_configs: Vec<SgRouteFilter>) -> TardisResult<Vec<(String, Box<dyn SgPluginFilter>)>> {
    let mut plugin_filters: Vec<(String, Box<dyn SgPluginFilter>)> = Vec::new();
    for filter_conf in filter_configs {
        let name = filter_conf.name.unwrap_or(TardisFuns::field.nanoid());
        let filter_def = get_filter_def(&filter_conf.code);
        let filter_inst = filter_def.new(filter_conf.spec)?;
        plugin_filters.push((format!("{name}_header_modifier"), filter_inst));
    }
    for (_, plugin_filter) in &plugin_filters {
        plugin_filter.init().await?;
    }
    Ok(plugin_filters)
}

pub trait SgPluginFilterDef {
    fn new(&self, spec: Value) -> TardisResult<Box<dyn SgPluginFilter>>;
}

#[async_trait]
pub trait SgPluginFilter: Send + Sync + 'static {
    fn kind(&self) -> SgPluginFilterKind;

    async fn init(&self) -> TardisResult<()>;

    async fn destroy(&self) -> TardisResult<()>;

    async fn req_filter(&self, mut ctx: SgRouteFilterContext) -> TardisResult<(bool, SgRouteFilterContext)>;

    async fn resp_filter(&self, mut ctx: SgRouteFilterContext) -> TardisResult<(bool, SgRouteFilterContext)>;
}

#[derive(Debug, Clone)]
pub enum SgPluginFilterKind {
    Http,
    Grpc,
    Ws,
}

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
    mod_resp_status_code: Option<StatusCode>,
    mod_resp_headers: Option<HeaderMap<HeaderValue>>,
    mod_resp_body: Option<Vec<u8>>,

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
            mod_resp_status_code: None,
            mod_resp_headers: None,
            mod_resp_body: None,
            ext: HashMap::new(),
            action: SgRouteFilterRequestAction::None,
            gateway_name: gateway_name,
        }
    }

    pub fn resp(mut self, status_code: StatusCode, headers: HeaderMap<HeaderValue>, body: Body) -> Self {
        self.raw_resp_status_code = status_code;
        self.raw_resp_headers = headers;
        self.raw_resp_body = Some(body);
        self
    }

    pub fn get_req_method(&mut self) -> &Method {
        if self.mod_req_method.is_none() {
            self.mod_req_method = Some(self.raw_req_method.clone());
        }
        &self.mod_req_method.as_ref().unwrap()
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
        &self.mod_req_uri.as_ref().unwrap()
    }

    pub fn set_req_uri(&mut self, uri: Uri) {
        self.mod_req_uri = Some(uri);
    }

    pub fn get_req_uri_raw(&self) -> &Uri {
        &self.raw_req_uri
    }

    pub fn get_req_version(&mut self) -> &Version {
        if self.mod_req_version.is_none() {
            self.mod_req_version = Some(self.raw_req_version.clone());
        }
        &self.mod_req_version.as_ref().unwrap()
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
        &self.mod_req_headers.as_ref().unwrap()
    }

    pub fn set_req_headers(&mut self, req_headers: HeaderMap<HeaderValue>) {
        self.mod_req_headers = Some(req_headers);
    }

    pub fn set_req_header(&mut self, key: &str, value: &str) -> TardisResult<()> {
        if self.mod_req_headers.is_none() {
            self.mod_req_headers = Some(self.raw_req_headers.clone());
        }
        let mod_req_headers = self.mod_req_headers.as_mut().unwrap();
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
            unsafe { std::mem::swap(&mut body, &mut self.mod_req_body) };
            Ok(body)
        } else if self.raw_req_body.is_some() {
            let mut body = None;
            unsafe { std::mem::swap(&mut body, &mut self.raw_req_body) };
            let body = hyper::body::to_bytes(body.unwrap()).await.map_err(|error| TardisError::format_error(&format!("[SG.Filter] Request Body parsing error:{error}"), ""))?;
            let body = body.iter().rev().cloned().collect::<Vec<u8>>();
            Ok(Some(body))
        } else {
            Ok(None)
        }
    }

    pub fn set_req_body(&mut self, body: Vec<u8>) {
        self.set_req_header("Content-Length", body.len().to_string().as_str());
        self.mod_req_body = Some(body);
    }

    pub fn pop_req_body_raw(&mut self) -> TardisResult<Option<Body>> {
        if self.mod_req_body.is_some() {
            let mut body = None;
            unsafe { std::mem::swap(&mut body, &mut self.mod_req_body) };
            Ok(body.map(|b| Body::from(b)))
        } else if self.raw_req_body.is_some() {
            let mut body = None;
            unsafe { std::mem::swap(&mut body, &mut self.raw_req_body) };
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
            self.mod_resp_status_code = Some(self.raw_resp_status_code.clone());
        }
        &self.mod_resp_status_code.as_ref().unwrap()
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
        &self.mod_resp_headers.as_ref().unwrap()
    }

    pub fn set_resp_headers(&mut self, resp_headers: HeaderMap<HeaderValue>) {
        self.mod_resp_headers = Some(resp_headers);
    }

    pub fn set_resp_header(&mut self, key: &str, value: &str) -> TardisResult<()> {
        if self.mod_resp_headers.is_none() {
            self.mod_resp_headers = Some(self.raw_resp_headers.clone());
        }
        let mod_resp_headers = self.mod_resp_headers.as_mut().unwrap();
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
            unsafe { std::mem::swap(&mut body, &mut self.mod_resp_body) };
            Ok(body)
        } else if self.raw_resp_body.is_some() {
            let mut body = None;
            unsafe { std::mem::swap(&mut body, &mut self.raw_resp_body) };
            let body = hyper::body::to_bytes(body.unwrap()).await.map_err(|error| TardisError::format_error(&format!("[SG.Filter] Response Body parsing error:{error}"), ""))?;
            let body = body.iter().rev().cloned().collect::<Vec<u8>>();
            Ok(Some(body))
        } else {
            Ok(None)
        }
    }

    pub fn set_resp_body(&mut self, body: Vec<u8>) {
        self.set_resp_header("Content-Length", body.len().to_string().as_str());
        self.mod_resp_body = Some(body);
    }

    pub fn pop_resp_body_raw(&mut self) -> TardisResult<Option<Body>> {
        if self.mod_resp_body.is_some() {
            let mut body = None;
            unsafe { std::mem::swap(&mut body, &mut self.mod_resp_body) };
            Ok(body.map(|b| Body::from(b)))
        } else if self.raw_resp_body.is_some() {
            let mut body = None;
            unsafe { std::mem::swap(&mut body, &mut self.raw_resp_body) };
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
        crate::functions::cache::get(&self.gateway_name)
    }
}
