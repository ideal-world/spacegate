use http::{HeaderMap, HeaderName, HeaderValue, Method, Response, StatusCode, Uri, Version};
use hyper::Body;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::net::SocketAddr;
use std::ops::{Deref, DerefMut};
use tardis::basic::error::TardisError;
use tardis::basic::result::TardisResult;

use tardis::TardisFuns;

use crate::config::gateway_dto::SgProtocol;

use crate::instance::{SgBackendInst, SgHttpRouteMatchInst, SgHttpRouteRuleInst};

use super::filters::SgPluginFilterKind;

/// Chosen http route rule
#[derive(Default, Debug)]
pub struct ChosenHttpRouteRuleInst {
    matched_match: Option<SgHttpRouteMatchInst>,
    available_backends: Vec<AvailableBackendInst>,
    timeout_ms: Option<u64>,
}

impl ChosenHttpRouteRuleInst {
    pub fn clone_from(chose_route_rule: &SgHttpRouteRuleInst, matched_match_inst: Option<&SgHttpRouteMatchInst>) -> Self {
        Self {
            matched_match: matched_match_inst.cloned(),
            available_backends: chose_route_rule.backends.as_ref().map(|bs| bs.iter().map(AvailableBackendInst::clone_from).collect::<Vec<_>>()).unwrap_or_default(),
            timeout_ms: chose_route_rule.timeout_ms,
        }
    }
}

/// Same as `SgBackendInst`,  but it emphasizes that the backend is available for
/// use in the current request.
#[derive(Default, Debug, Clone)]
pub struct AvailableBackendInst {
    pub name_or_host: String,
    pub namespace: Option<String>,
    pub port: u16,
    pub timeout_ms: Option<u64>,
    pub protocol: Option<SgProtocol>,
    pub weight: Option<u16>,
}

impl AvailableBackendInst {
    fn clone_from(value: &SgBackendInst) -> Self {
        Self {
            name_or_host: value.name_or_host.clone(),
            namespace: value.namespace.clone(),
            port: value.port,
            timeout_ms: value.timeout_ms,
            protocol: value.protocol.clone(),
            weight: value.weight,
        }
    }

    pub fn get_base_url(&self) -> String {
        let scheme = self.protocol.as_ref().unwrap_or(&SgProtocol::Http);
        let host = format!("{}{}", self.name_or_host, self.namespace.as_ref().map(|n| format!(".{n}")).unwrap_or("".to_string()));
        let port = if (self.port == 0 || self.port == 80) && scheme == &SgProtocol::Http || (self.port == 0 || self.port == 443) && scheme == &SgProtocol::Https {
            "".to_string()
        } else {
            format!(":{}", self.port)
        };
        format!("{}://{}{}", scheme, host, port)
    }
}

/// `SgRouteFilterRequestAction` represents the action to be taken after executing
/// plugin request filtering, as configured by the plugin.
#[derive(Debug, PartialEq, Eq, Clone)]
pub enum SgRouteFilterRequestAction {
    None,
    /// Forwarding the current request.
    Redirect,
    /// Constructing a response directly based on the information in the context , without making a backend request.
    Response,
}

#[derive(Debug)]
pub struct MaybeModified<T> {
    raw: T,
    modified: Option<T>,
}

impl<T> MaybeModified<T> {
    pub fn new(value: T) -> Self {
        Self { raw: value, modified: None }
    }
    pub fn reset(&mut self, value: T) {
        self.raw = value;
        self.modified.take();
    }
    #[inline]
    pub fn get_raw(&self) -> &T {
        &self.raw
    }
    #[inline]
    pub fn get(&self) -> &T {
        self.modified.as_ref().unwrap_or(&self.raw)
    }
    #[inline]
    pub fn replace(&mut self, val: T) -> Option<T> {
        self.modified.replace(val)
    }
    #[inline]
    pub fn set(&mut self, val: T) {
        self.modified.replace(val);
    }
    #[inline]
    pub fn get_modified_mut(&mut self) -> Option<&mut T> {
        self.modified.as_mut()
    }
    #[inline]
    pub fn is_modified(&self) -> bool {
        self.modified.is_some()
    }
}

impl<T: Clone> MaybeModified<T> {
    pub fn get_mut(&mut self) -> &mut T {
        self.modified.get_or_insert(self.raw.clone())
    }
}

impl<T> From<T> for MaybeModified<T> {
    fn from(value: T) -> Self {
        MaybeModified::new(value)
    }
}

impl<T> Deref for MaybeModified<T> {
    type Target = T;
    fn deref(&self) -> &Self::Target {
        self.get()
    }
}

impl<T: Clone> DerefMut for MaybeModified<T> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        self.get_mut()
    }
}

#[derive(Debug)]
pub struct SgCtxRequest {
    pub method: MaybeModified<Method>,
    pub uri: MaybeModified<Uri>,
    pub version: MaybeModified<Version>,
    pub body: Body,
    pub headers: MaybeModified<HeaderMap<HeaderValue>>,
    pub remote_addr: SocketAddr,
}

impl SgCtxRequest {
    pub fn new(method: Method, uri: Uri, version: Version, headers: HeaderMap<HeaderValue>, body: Body, remote_addr: SocketAddr) -> Self {
        Self {
            method: MaybeModified::new(method),
            uri: MaybeModified::new(uri),
            version: MaybeModified::new(version),
            body,
            headers: MaybeModified::new(headers),
            remote_addr,
        }
    }

    #[inline]
    pub fn get_method(&mut self) -> &Method {
        &self.method
    }

    #[inline]
    pub fn set_method(&mut self, method: Method) {
        self.method.set(method)
    }

    #[inline]
    pub fn get_method_raw(&self) -> &Method {
        self.method.get_raw()
    }

    #[inline]
    pub fn get_uri(&mut self) -> &Uri {
        &self.uri
    }

    #[inline]
    pub fn set_uri(&mut self, uri: Uri) {
        self.uri.set(uri)
    }

    #[inline]
    pub fn get_uri_raw(&self) -> &Uri {
        self.uri.get_raw()
    }

    #[inline]
    pub fn get_version(&mut self) -> &Version {
        &self.version
    }

    #[inline]
    pub fn set_version(&mut self, version: Version) {
        self.version.set(version)
    }

    #[inline]
    pub fn get_version_raw(&self) -> &Version {
        self.version.get_raw()
    }

    #[inline]
    pub fn get_headers(&mut self) -> &HeaderMap<HeaderValue> {
        self.headers.get()
    }

    #[inline]
    pub fn get_headers_mut(&mut self) -> &mut HeaderMap<HeaderValue> {
        self.headers.get_mut()
    }

    #[inline]
    pub fn set_headers(&mut self, req_headers: HeaderMap<HeaderValue>) {
        self.headers.set(req_headers)
    }

    pub fn set_header_str(&mut self, key: &str, value: &str) -> TardisResult<()> {
        self.get_headers_mut().insert(
            HeaderName::try_from(key).map_err(|error| TardisError::format_error(&format!("[SG.Filter] Header key {key} parsing error: {error}"), ""))?,
            HeaderValue::try_from(value).map_err(|error| TardisError::format_error(&format!("[SG.Filter] Header value {value} parsing error: {error}"), ""))?,
        );
        Ok(())
    }

    pub fn set_header(&mut self, key: HeaderName, value: &str) -> TardisResult<()> {
        self.get_headers_mut().insert(
            key,
            HeaderValue::try_from(value).map_err(|error| TardisError::format_error(&format!("[SG.Filter] Header value {value} parsing error: {error}"), ""))?,
        );
        Ok(())
    }

    pub fn get_headers_raw(&self) -> &HeaderMap<HeaderValue> {
        self.headers.get_raw()
    }

    pub fn get_remote_addr(&self) -> &SocketAddr {
        &self.remote_addr
    }

    pub fn take_body(&mut self) -> Body {
        std::mem::take(&mut self.body)
    }

    pub fn replace_body(&mut self, body: impl Into<Body>) -> Body {
        std::mem::replace(&mut self.body, body.into())
    }

    #[inline]
    pub fn set_body(&mut self, body: impl Into<Body>) {
        let _ = self.replace_body(body);
    }

    /// it's a shortcut for [take_body](SgCtxRequest) + [hyper::body::to_bytes]
    pub async fn take_body_into_bytes(&mut self) -> TardisResult<hyper::body::Bytes> {
        let bytes = hyper::body::to_bytes(self.take_body()).await.map_err(|e| TardisError::format_error(&format!("[SG.Filter] fail to collect body into bytes: {e}"), ""))?;
        Ok(bytes)
    }

    /// it's a shortcut for [`take_body`](SgCtxRequest) + [hyper::body::aggregate]
    pub async fn take_body_into_buf(&mut self) -> TardisResult<impl hyper::body::Buf> {
        let buf = hyper::body::aggregate(self.take_body()).await.map_err(|e| TardisError::format_error(&format!("[SG.Filter] fail to aggregate body: {e}"), ""))?;
        Ok(buf)
    }

    /// # Performance
    /// this method will read all of the body and clone it, and it's body will become an once stream which holds the whole body.
    pub async fn dump_body(&mut self) -> TardisResult<hyper::body::Bytes> {
        let bytes = self.take_body_into_bytes().await?;
        self.set_body(bytes.clone());
        Ok(bytes)
    }
}

#[derive(Debug)]
pub struct SgCtxResponse {
    pub status_code: MaybeModified<StatusCode>,
    pub headers: MaybeModified<HeaderMap<HeaderValue>>,
    pub body: Body,
    resp_err: Option<TardisError>,
}

impl SgCtxResponse {
    pub fn new() -> Self {
        Self {
            status_code: MaybeModified::new(StatusCode::OK),
            headers: MaybeModified::new(HeaderMap::new()),
            body: Body::empty(),
            resp_err: None,
        }
    }

    #[inline]
    pub fn is_resp_error(&self) -> bool {
        self.resp_err.is_some()
    }

    #[inline]
    pub fn get_status_code(&mut self) -> &StatusCode {
        self.status_code.get()
    }

    #[inline]
    pub fn set_status_code(&mut self, status_code: StatusCode) {
        self.status_code.set(status_code)
    }

    #[inline]
    pub fn get_status_code_raw(&self) -> &StatusCode {
        self.status_code.get_raw()
    }

    #[inline]
    pub fn get_headers(&mut self) -> &HeaderMap<HeaderValue> {
        self.headers.get()
    }

    #[inline]
    pub fn get_headers_raw(&mut self) -> &HeaderMap<HeaderValue> {
        self.headers.get_raw()
    }

    #[inline]
    pub fn get_headers_mut(&mut self) -> &mut HeaderMap<HeaderValue> {
        self.headers.get_mut()
    }

    #[inline]
    pub fn set_headers(&mut self, req_headers: HeaderMap<HeaderValue>) {
        self.headers.set(req_headers)
    }

    pub fn set_header_str(&mut self, key: &str, value: &str) -> TardisResult<()> {
        self.get_headers_mut().insert(
            HeaderName::try_from(key).map_err(|error| TardisError::format_error(&format!("[SG.Filter] Header key {key} parsing error: {error}"), ""))?,
            HeaderValue::try_from(value).map_err(|error| TardisError::format_error(&format!("[SG.Filter] Header value {value} parsing error: {error}"), ""))?,
        );
        Ok(())
    }

    pub fn set_header(&mut self, key: HeaderName, value: &str) -> TardisResult<()> {
        self.get_headers_mut().insert(
            key,
            HeaderValue::try_from(value).map_err(|error| TardisError::format_error(&format!("[SG.Filter] Header value {value} parsing error: {error}"), ""))?,
        );
        Ok(())
    }

    pub fn remove_header(&mut self, key: HeaderName) -> TardisResult<()> {
        if let Some(headers) = self.headers.get_modified_mut() {
            headers.remove(key);
        }
        Ok(())
    }

    pub fn remove_header_str(&mut self, key: &str) -> TardisResult<()> {
        if let Some(headers) = self.headers.get_modified_mut() {
            headers.remove(HeaderName::try_from(key).map_err(|error| TardisError::format_error(&format!("[SG.Filter] Header key {key} parsing error: {error}"), ""))?);
        }
        Ok(())
    }

    #[inline]
    pub fn take_body(&mut self) -> Body {
        std::mem::take(&mut self.body)
    }

    #[inline]
    pub fn replace_body(&mut self, body: impl Into<Body>) -> Body {
        std::mem::replace(&mut self.body, body.into())
    }

    #[inline]
    pub fn set_body(&mut self, body: impl Into<Body>) {
        let _ = self.replace_body(body);
    }

    /// it's a shortcut for [take_body](SgCtxResponse) + [hyper::body::to_bytes]
    pub async fn take_body_into_bytes(&mut self) -> TardisResult<hyper::body::Bytes> {
        let bytes = hyper::body::to_bytes(self.take_body()).await.map_err(|e| TardisError::format_error(&format!("[SG.Filter] fail to collect body into bytes: {e}"), ""))?;
        Ok(bytes)
    }

    /// it's a shortcut for [take_body](SgCtxResponse) + [hyper::body::aggregate]
    pub async fn take_body_into_buf(&mut self) -> TardisResult<impl hyper::body::Buf> {
        let buf = hyper::body::aggregate(self.take_body()).await.map_err(|e| TardisError::format_error(&format!("[SG.Filter] fail to aggregate body: {e}"), ""))?;
        Ok(buf)
    }

    /// # Performance
    /// This method will read **all** of the body and **clone** it, and it's body will become an once stream which holds the whole body.
    pub async fn dump_body(&mut self) -> TardisResult<hyper::body::Bytes> {
        let bytes = self.take_body_into_bytes().await?;
        self.set_body(bytes.clone());
        Ok(bytes)
    }
}

impl Default for SgCtxResponse {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Debug, PartialEq, Eq, Clone, Serialize, Deserialize)]
pub struct SGIdentInfo {
    pub id: String,
    pub name: Option<String>,
    pub roles: Vec<SGRoleInfo>,
}

#[derive(Debug, PartialEq, Eq, Clone, Serialize, Deserialize)]
pub struct SGRoleInfo {
    pub id: String,
    pub name: Option<String>,
}

#[derive(Debug)]
pub struct SgRoutePluginContext {
    request_id: String,
    request_kind: SgPluginFilterKind,

    pub request: SgCtxRequest,
    pub response: SgCtxResponse,

    chosen_route_rule: Option<ChosenHttpRouteRuleInst>,
    chosen_backend: Option<AvailableBackendInst>,

    ext: HashMap<String, String>,

    /// Describe user information
    ident_info: Option<SGIdentInfo>,
    action: SgRouteFilterRequestAction,
    gateway_name: String,
}

#[allow(dead_code)]
impl SgRoutePluginContext {
    pub fn new_http(
        method: Method,
        uri: Uri,
        version: Version,
        headers: HeaderMap<HeaderValue>,
        body: Body,
        remote_addr: SocketAddr,
        gateway_name: String,
        chose_route_rule: Option<ChosenHttpRouteRuleInst>,
    ) -> Self {
        Self {
            request_id: TardisFuns::field.nanoid(),
            request: SgCtxRequest::new(method, uri, version, headers, body, remote_addr),
            response: SgCtxResponse::new(),
            ext: HashMap::new(),
            action: SgRouteFilterRequestAction::None,
            gateway_name,
            chosen_route_rule: chose_route_rule,
            chosen_backend: None,
            request_kind: SgPluginFilterKind::Http,
            ident_info: None,
        }
    }

    pub fn new_ws(
        method: Method,
        uri: Uri,
        version: Version,
        headers: HeaderMap<HeaderValue>,
        remote_addr: SocketAddr,
        gateway_name: String,
        chose_route_rule: Option<ChosenHttpRouteRuleInst>,
    ) -> Self {
        Self {
            request_id: TardisFuns::field.nanoid(),
            request: SgCtxRequest::new(method, uri, version, headers, Body::default(), remote_addr),
            response: SgCtxResponse::new(),
            ext: HashMap::new(),
            action: SgRouteFilterRequestAction::None,
            gateway_name,
            chosen_route_rule: chose_route_rule,
            chosen_backend: None,
            request_kind: SgPluginFilterKind::Ws,
            ident_info: None,
        }
    }

    /// The following two methods can only be used to fill in the context [resp] [resp_from_error]
    pub fn resp(mut self, status_code: StatusCode, headers: HeaderMap<HeaderValue>, body: Body) -> Self {
        self.response.status_code.reset(status_code);
        self.response.headers.reset(headers);
        self.response.body = body;
        self.response.resp_err = None;
        self
    }

    pub fn resp_from_error(mut self, error: TardisError) -> Self {
        self.response.resp_err = Some(error);
        self.response.status_code.reset(StatusCode::BAD_GATEWAY);
        self
    }

    pub fn is_resp_error(&self) -> bool {
        self.response.is_resp_error()
    }

    pub fn get_request_id(&self) -> &str {
        &self.request_id
    }

    pub fn get_request_kind(&self) -> &SgPluginFilterKind {
        &self.request_kind
    }

    /// build response from Context
    pub async fn build_response(&mut self) -> TardisResult<Response<Body>> {
        if let Some(err) = &self.response.resp_err {
            return Err(err.clone());
        }
        let mut resp = Response::builder();
        for (k, v) in self.response.get_headers() {
            resp = resp.header(
                k.as_str(),
                v.to_str().map_err(|_| TardisError::bad_request(&format!("[SG.Route] header {k}'s value illegal: is not ascii"), ""))?.to_string(),
            );
        }
        let resp = resp
            .status(self.response.get_status_code())
            .body(self.response.take_body())
            .map_err(|error| TardisError::internal_error(&format!("[SG.Route] Build response error:{error}"), ""))?;
        Ok(resp)
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
        if action == SgRouteFilterRequestAction::Redirect || action == SgRouteFilterRequestAction::Response {
            self.chosen_backend = None;
        }
        self.action = action;
    }

    pub fn set_chose_backend(&mut self, chose_backend: &SgBackendInst) {
        self.chosen_backend = Some(AvailableBackendInst::clone_from(chose_backend));
    }

    pub fn get_chose_backend_name(&self) -> Option<String> {
        self.chosen_backend.clone().map(|b| b.name_or_host)
    }

    pub fn get_available_backend(&self) -> Vec<&AvailableBackendInst> {
        self.chosen_route_rule.as_ref().map(|r| r.available_backends.iter().collect()).unwrap_or_default()
    }

    pub fn get_timeout_ms(&self) -> Option<u64> {
        if let Some(timeout) = self.chosen_backend.as_ref().and_then(|b| b.timeout_ms) {
            Some(timeout)
        } else {
            self.chosen_route_rule.as_ref().and_then(|r| r.timeout_ms)
        }
    }

    pub fn get_rule_matched(&self) -> Option<SgHttpRouteMatchInst> {
        self.chosen_route_rule.as_ref().and_then(|r| r.matched_match.clone())
    }

    pub fn get_gateway_name(&self) -> String {
        self.gateway_name.clone()
    }

    pub fn get_cert_info(&self) -> Option<&SGIdentInfo> {
        self.ident_info.as_ref()
    }

    pub fn set_cert_info(&mut self, cert_info: SGIdentInfo) {
        self.ident_info = Some(cert_info);
    }

    #[cfg(feature = "cache")]
    pub fn cache(&self) -> TardisResult<&'static tardis::cache::cache_client::TardisCacheClient> {
        crate::functions::cache_client::get(&self.gateway_name)
    }
}
