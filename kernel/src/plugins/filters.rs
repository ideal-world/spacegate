pub mod header_modifier;
pub mod redirect;
use async_trait::async_trait;
use http::{HeaderMap, HeaderValue, Method, Request, StatusCode, Uri, Version};
use hyper::Body;
use std::collections::HashMap;
use tardis::basic::error::TardisError;
use tardis::basic::result::TardisResult;

use crate::config::plugin_filter_dto::SgRouteFilter;

static mut FILTERS: Option<HashMap<String, Vec<Box<dyn SgPluginFilter>>>> = None;

pub async fn init(name: &str, route_confs: Vec<SgRouteFilter>) -> TardisResult<()> {
    let mut plugin_filters: Vec<Box<dyn SgPluginFilter>> = Vec::new();
    for route in route_confs {
        if let Some(header_modifier) = route.header_modifier {
            plugin_filters.push(Box::new(header_modifier))
        }
        if let Some(redirect) = route.redirect {
            plugin_filters.push(Box::new(redirect))
        }
    }
    unsafe {
        if FILTERS.is_none() {
            FILTERS = Some(HashMap::new());
        }
        FILTERS.as_mut().unwrap().insert(name.to_string(), plugin_filters);
    }
    Ok(())
}

pub async fn remove(name: &str) -> TardisResult<()> {
    unsafe {
        if FILTERS.is_none() {
            FILTERS = Some(HashMap::new());
        }
        FILTERS.as_mut().unwrap().remove(name);
    }
    Ok(())
}

pub fn get(name: &str) -> TardisResult<&'static Vec<Box<dyn SgPluginFilter>>> {
    unsafe {
        if let Some(filters) = FILTERS.as_ref().unwrap().get(name) {
            Ok(filters)
        } else {
            Err(TardisError::bad_request(&format!("[SG.server] Get filters by gateway {name} failed"), ""))
        }
    }
}

#[async_trait]
pub trait SgPluginFilter: Send + Sync + 'static {
    fn kind(&self) -> SgPluginFilterKind;

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
    pub method: Method,
    pub uri: Uri,
    pub version: Version,
    pub req_headers: HeaderMap<HeaderValue>,
    pub resp_headers: Option<HeaderMap<HeaderValue>>,
    pub resp_status_code: Option<StatusCode>,
    pub ext: HashMap<String, String>,
    inner_body: Option<Vec<u8>>,
    raw_req: Option<Request<Body>>,
}

impl SgRouteFilterContext {
    pub async fn into_body(self) -> TardisResult<Self> {
        if self.inner_body.is_some() {
            return Ok(self);
        }
        if let Some(raw_req) = self.raw_req {
            let whole_body =
                hyper::body::to_bytes(raw_req.into_body()).await.map_err(|error| TardisError::format_error(&format!("[SG.filter] Request Body parsing error:{error}"), ""))?;
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
