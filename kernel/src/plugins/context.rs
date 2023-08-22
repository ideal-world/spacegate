use http::{HeaderMap, HeaderName, HeaderValue, Method, Response, StatusCode, Uri, Version};
use hyper::Body;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::net::SocketAddr;
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
pub struct SgCtxRequest {
    raw_method: Method,
    raw_uri: Uri,
    raw_version: Version,
    raw_body: Option<Body>,
    raw_headers: HeaderMap<HeaderValue>,
    raw_remote_addr: SocketAddr,

    mod_method: Option<Method>,
    mod_uri: Option<Uri>,
    mod_version: Option<Version>,
    mod_body: Option<Vec<u8>>,
    mod_headers: Option<HeaderMap<HeaderValue>>,
}

impl SgCtxRequest {
    pub fn new(method: Method, uri: Uri, version: Version, headers: HeaderMap<HeaderValue>, body: Option<Body>, remote_addr: SocketAddr) -> Self {
        Self {
            raw_method: method,
            raw_uri: uri,
            raw_version: version,
            raw_body: body,
            raw_headers: headers,
            raw_remote_addr: remote_addr,
            mod_method: None,
            mod_uri: None,
            mod_version: None,
            mod_body: None,
            mod_headers: None,
        }
    }

    pub fn get_method(&mut self) -> &Method {
        if self.mod_method.is_none() {
            self.mod_method = Some(self.raw_method.clone());
        }
        self.mod_method.as_ref().expect("Unreachable code")
    }

    pub fn set_method(&mut self, method: Method) {
        self.mod_method = Some(method);
    }

    pub fn get_method_raw(&self) -> &Method {
        &self.raw_method
    }

    pub fn get_uri(&mut self) -> &Uri {
        if self.mod_uri.is_none() {
            self.mod_uri = Some(self.raw_uri.clone());
        }
        self.mod_uri.as_ref().expect("Unreachable code")
    }

    pub fn set_uri(&mut self, uri: Uri) {
        self.mod_uri = Some(uri);
    }

    pub fn get_uri_raw(&self) -> &Uri {
        &self.raw_uri
    }

    pub fn get_version(&mut self) -> &Version {
        if self.mod_version.is_none() {
            self.mod_version = Some(self.raw_version);
        }
        self.mod_version.as_ref().expect("Unreachable code")
    }

    pub fn set_version(&mut self, version: Version) {
        self.mod_version = Some(version);
    }

    pub fn get_version_raw(&self) -> &Version {
        &self.raw_version
    }

    pub fn get_headers(&mut self) -> &HeaderMap<HeaderValue> {
        if self.mod_headers.is_none() {
            self.mod_headers = Some(self.raw_headers.clone());
        }
        self.mod_headers.as_ref().expect("Unreachable code")
    }

    pub fn get_headers_mut(&mut self) -> &mut HeaderMap<HeaderValue> {
        if self.mod_headers.is_none() {
            self.mod_headers = Some(self.raw_headers.clone());
        }
        self.mod_headers.as_mut().expect("Unreachable code")
    }

    pub fn set_headers(&mut self, req_headers: HeaderMap<HeaderValue>) {
        self.mod_headers = Some(req_headers);
    }

    pub fn set_header(&mut self, key: &str, value: &str) -> TardisResult<()> {
        if self.mod_headers.is_none() {
            self.mod_headers = Some(self.raw_headers.clone());
        }
        let mod_headers = self.mod_headers.as_mut().expect("Unreachable code");
        mod_headers.insert(
            HeaderName::try_from(key).map_err(|error| TardisError::format_error(&format!("[SG.Filter] Header key {key} parsing error: {error}"), ""))?,
            HeaderValue::try_from(value).map_err(|error| TardisError::format_error(&format!("[SG.Filter] Header value {value} parsing error: {error}"), ""))?,
        );
        Ok(())
    }

    pub fn get_headers_raw(&self) -> &HeaderMap<HeaderValue> {
        &self.raw_headers
    }

    pub async fn pop_body(&mut self) -> TardisResult<Option<Vec<u8>>> {
        if self.mod_body.is_some() {
            let mut body = None;
            std::mem::swap(&mut body, &mut self.mod_body);
            Ok(body)
        } else if self.raw_body.is_some() {
            let mut body = None;
            std::mem::swap(&mut body, &mut self.raw_body);
            let body = hyper::body::to_bytes(body.expect("Unreachable code"))
                .await
                .map_err(|error| TardisError::format_error(&format!("[SG.Filter] Request Body parsing error:{error}"), ""))?;
            let body = body.iter().cloned().collect::<Vec<u8>>();
            Ok(Some(body))
        } else {
            Ok(None)
        }
    }

    pub fn set_body(&mut self, body: Vec<u8>) -> TardisResult<()> {
        self.get_headers_mut().remove(http::header::TRANSFER_ENCODING.as_str());
        self.set_header(http::header::CONTENT_LENGTH.as_str(), body.len().to_string().as_str())?;
        self.mod_body = Some(body);
        Ok(())
    }

    pub fn pop_body_raw(&mut self) -> TardisResult<Option<Body>> {
        if self.mod_body.is_some() {
            let mut body = None;
            std::mem::swap(&mut body, &mut self.mod_body);
            Ok(body.map(Body::from))
        } else if self.raw_body.is_some() {
            let mut body = None;
            std::mem::swap(&mut body, &mut self.raw_body);
            Ok(body)
        } else {
            Ok(None)
        }
    }

    pub fn get_remote_addr(&self) -> &SocketAddr {
        &self.raw_remote_addr
    }
}

#[derive(Debug)]
pub struct SgCtxResponse {
    raw_status_code: StatusCode,
    raw_headers: HeaderMap<HeaderValue>,
    raw_body: Option<Body>,
    raw_resp_err: Option<TardisError>,
    mod_status_code: Option<StatusCode>,
    mod_headers: Option<HeaderMap<HeaderValue>>,
    mod_body: Option<Vec<u8>>,
}

impl SgCtxResponse {
    pub fn new() -> Self {
        Self {
            raw_status_code: StatusCode::OK,
            raw_headers: HeaderMap::new(),
            raw_body: None,
            raw_resp_err: None,
            mod_status_code: None,
            mod_headers: None,
            mod_body: None,
        }
    }

    pub fn is_resp_error(&self) -> bool {
        self.raw_resp_err.is_some()
    }

    pub fn get_status_code(&mut self) -> &StatusCode {
        if self.mod_status_code.is_none() {
            self.mod_status_code = Some(self.raw_status_code);
        }
        self.mod_status_code.as_ref().expect("Unreachable code")
    }

    pub fn set_status_code(&mut self, status_code: StatusCode) {
        self.mod_status_code = Some(status_code);
    }

    pub fn get_status_code_raw(&self) -> &StatusCode {
        &self.raw_status_code
    }

    pub fn get_headers(&mut self) -> &HeaderMap<HeaderValue> {
        if self.mod_headers.is_none() {
            self.mod_headers = Some(self.raw_headers.clone());
        }
        self.mod_headers.as_ref().expect("Unreachable code")
    }

    pub fn get_headers_mut(&mut self) -> &mut HeaderMap<HeaderValue> {
        if self.mod_headers.is_none() {
            self.mod_headers = Some(self.raw_headers.clone());
        }
        self.mod_headers.as_mut().expect("Unreachable code")
    }

    pub fn set_headers(&mut self, resp_headers: HeaderMap<HeaderValue>) {
        self.mod_headers = Some(resp_headers);
    }

    pub fn set_header(&mut self, key: &str, value: &str) -> TardisResult<()> {
        if self.mod_headers.is_none() {
            self.mod_headers = Some(self.raw_headers.clone());
        }
        let mod_headers = self.mod_headers.as_mut().expect("Unreachable code");
        mod_headers.insert(
            HeaderName::try_from(key).map_err(|error| TardisError::format_error(&format!("[SG.Filter] Header key {key} parsing error: {error}"), ""))?,
            HeaderValue::try_from(value).map_err(|error| TardisError::format_error(&format!("[SG.Filter] Header value {value} parsing error: {error}"), ""))?,
        );
        Ok(())
    }

    pub fn remove_header(&mut self, key: &str) -> TardisResult<()> {
        if let Some(headers) = self.mod_headers.as_mut() {
            headers.remove(HeaderName::try_from(key).map_err(|error| TardisError::format_error(&format!("[SG.Filter] Header key {key} parsing error: {error}"), ""))?);
        }
        Ok(())
    }

    pub fn get_headers_raw(&self) -> &HeaderMap<HeaderValue> {
        &self.raw_headers
    }

    pub async fn pop_body(&mut self) -> TardisResult<Option<Vec<u8>>> {
        if self.mod_body.is_some() {
            let mut body = None;
            std::mem::swap(&mut body, &mut self.mod_body);
            Ok(body)
        } else if self.raw_body.is_some() {
            let mut body = None;
            std::mem::swap(&mut body, &mut self.raw_body);
            let body = hyper::body::to_bytes(body.expect("Unreachable code"))
                .await
                .map_err(|error| TardisError::format_error(&format!("[SG.Filter] Response Body parsing error:{error}"), ""))?;
            let body = body.iter().cloned().collect::<Vec<u8>>();
            Ok(Some(body))
        } else {
            Ok(None)
        }
    }

    pub fn set_body(&mut self, body: Vec<u8>) -> TardisResult<()> {
        self.get_headers_mut().remove(http::header::TRANSFER_ENCODING.as_str());
        self.set_header(http::header::CONTENT_LENGTH.as_str(), body.len().to_string().as_str())?;
        self.mod_body = Some(body);
        Ok(())
    }

    pub fn pop_body_raw(&mut self) -> TardisResult<Option<Body>> {
        if self.mod_body.is_some() {
            let mut body = None;
            std::mem::swap(&mut body, &mut self.mod_body);
            Ok(body.map(Body::from))
        } else if self.raw_body.is_some() {
            let mut body = None;
            std::mem::swap(&mut body, &mut self.raw_body);
            Ok(body)
        } else {
            Ok(None)
        }
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
            request: SgCtxRequest::new(method, uri, version, headers, Some(body), remote_addr),
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
            request: SgCtxRequest::new(method, uri, version, headers, None, remote_addr),
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

    ///The following two methods can only be used to fill in the context [resp] [resp_from_error]
    pub fn resp(mut self, status_code: StatusCode, headers: HeaderMap<HeaderValue>, body: Body) -> Self {
        self.response.raw_status_code = status_code;
        self.response.raw_headers = headers;
        self.response.raw_body = Some(body);
        self.response.raw_resp_err = None;
        self
    }

    pub fn resp_from_error(mut self, error: TardisError) -> Self {
        self.response.raw_resp_err = Some(error);
        self.response.raw_status_code = StatusCode::BAD_GATEWAY;
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
        if let Some(err) = &self.response.raw_resp_err {
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
            .body(Body::from(self.response.pop_body().await?.unwrap_or_default()))
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
