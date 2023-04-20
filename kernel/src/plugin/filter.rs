pub mod header_modifier;
pub mod redirect;
use std::collections::HashMap;

use async_trait::async_trait;
use http::{HeaderMap, HeaderValue, Method, Request, StatusCode, Uri, Version};
use http_body_util::BodyExt;
use hyper::body::{Body, Incoming};
use serde::{Deserialize, Serialize};
use tardis::basic::error::TardisError;
use tardis::basic::result::TardisResult;

/// RouteFilter defines processing steps that must be completed during the request or response lifecycle.
#[derive(Default, Debug, Serialize, Deserialize, Clone)]
pub struct SgRouteFilter {
    /// Type identifies the type of filter to apply.
    kind: Option<SgRouteFilterType>,
    /// HeaderModifier defines a schema for a header modifier filter.
    header_modifier: Option<header_modifier::SgFilerHeaderModifier>,
    /// Redirect defines a schema for a redirect filter.
    redirect: Option<redirect::SgFilerRedirect>,
}

/// RouteFilterType identifies a type of route filter.
#[derive(Debug, Serialize, Deserialize, Clone)]
pub enum SgRouteFilterType {
    HeaderModifier,
    Redirect,
}

#[derive(Debug, Clone)]
pub enum SgPluginFilterKind {
    Http,
    Grpc,
    Ws,
}

#[derive(Debug)]
pub struct SgRouteFilterContext {
    pub method: Method,
    pub uri: Uri,
    pub version: Version,
    pub req_headers: HeaderMap<HeaderValue>,
    pub resp_headers: Option<HeaderMap<HeaderValue>>,
    pub resp_status_code: Option<StatusCode>,
    pub ext: HashMap<String, String>,
    inner_body: Option<Vec<u8>>,
    raw_req: Option<Request<Incoming>>,
}

impl SgRouteFilterContext {
    pub fn body_size(&self) -> u64 {
        if let Some(body) = &self.inner_body {
            body.len() as u64
        } else {
            self.raw_req.as_ref().unwrap().body().size_hint().upper().unwrap_or(u64::MAX)
        }
    }

    pub async fn into_body(self) -> TardisResult<Self> {
        if self.inner_body.is_some() {
            return Ok(self);
        }
        if let Some(raw_req) = self.raw_req {
            let whole_body = raw_req.collect().await.map_err(|error| TardisError::format_error(&format!("[SG.filter] Request Body parsing error:{error}"), ""))?.to_bytes();
            let whole_body = whole_body.iter().rev().cloned().collect::<Vec<u8>>();
            Ok(Self {
                method: self.method,
                uri: self.uri,
                version: self.version,
                req_headers: self.req_headers,
                resp_headers: self.resp_headers,
                resp_status_code: self.resp_status_code,
                ext: self.ext,
                inner_body: Some(whole_body),
                raw_req: None,
            })
        } else {
            Ok(self)
        }
    }

    pub fn get_body(&self) -> Option<&Vec<u8>> {
        self.inner_body.as_ref()
    }

    pub fn set_body(&mut self, body: Vec<u8>) {
        self.inner_body = Some(body);
    }
}

#[async_trait]
pub trait SgPluginFilter {
    fn kind(&self) -> SgPluginFilterKind;

    async fn req_filter(&self, mut ctx: SgRouteFilterContext) -> TardisResult<(bool, SgRouteFilterContext)>;

    async fn resp_filter(&self, mut ctx: SgRouteFilterContext) -> TardisResult<(bool, SgRouteFilterContext)>;
}
