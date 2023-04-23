pub mod header_modifier;
pub mod redirect;
use async_trait::async_trait;
use http::{HeaderMap, HeaderValue, Method, StatusCode, Uri, Version};
use hyper::Body;
use std::collections::HashMap;
use std::net::SocketAddr;
use tardis::basic::error::TardisError;
use tardis::basic::result::TardisResult;
use tardis::TardisFuns;

use crate::config::plugin_filter_dto::SgRouteFilter;

pub async fn init(filter_configs: Vec<SgRouteFilter>) -> TardisResult<Vec<(String, Box<dyn SgPluginFilter>)>> {
    let mut plugin_filters: Vec<(String, Box<dyn SgPluginFilter>)> = Vec::new();
    for filter in filter_configs {
        let name = filter.name.unwrap_or(TardisFuns::field.nanoid());
        if let Some(header_modifier) = filter.header_modifier {
            plugin_filters.push((format!("{name}_header_modifier"), Box::new(header_modifier)))
        }
        if let Some(redirect) = filter.redirect {
            plugin_filters.push((format!("{name}_redirect"), Box::new(redirect)))
        }
    }
    for (_, plugin_filter) in &plugin_filters {
        plugin_filter.init().await?;
    }
    Ok(plugin_filters)
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
    pub method: Method,
    pub uri: Uri,
    pub version: Version,
    pub req_headers: HeaderMap<HeaderValue>,
    pub remote_addr: SocketAddr,
    pub resp_headers: Option<HeaderMap<HeaderValue>>,
    pub resp_status_code: Option<StatusCode>,
    pub ext: HashMap<String, String>,
    inner_body: Option<Vec<u8>>,
    raw_body: Option<Body>,
    pub action: SgRouteFilterRequestAction,
}

#[derive(Debug)]
pub enum SgRouteFilterRequestAction {
    None,
    Redirect,
    Response,
}

impl SgRouteFilterContext {
    pub async fn into_body(self) -> TardisResult<Self> {
        if self.raw_body.is_none() {
            return Ok(self);
        }
        if let Some(body) = self.raw_body {
            let whole_body = hyper::body::to_bytes(body).await.map_err(|error| TardisError::format_error(&format!("[SG.filter] Request Body parsing error:{error}"), ""))?;
            let whole_body = whole_body.iter().rev().cloned().collect::<Vec<u8>>();
            Ok(Self {
                method: self.method,
                uri: self.uri,
                version: self.version,
                req_headers: self.req_headers,
                remote_addr: self.remote_addr,
                resp_headers: self.resp_headers,
                resp_status_code: self.resp_status_code,
                ext: self.ext,
                inner_body: Some(whole_body),
                raw_body: None,
                action: self.action,
            })
        } else {
            return Ok(self);
        }
    }

    pub fn get_body(&self) -> Option<&Vec<u8>> {
        self.inner_body.as_ref()
    }

    pub fn set_body(&mut self, body: Vec<u8>) {
        self.inner_body = Some(body);
    }

    pub fn get_raw_body(self) -> Body {
        if let Some(raw_body) = self.raw_body {
            return raw_body;
        }
        if let Some(inner_body) = self.inner_body {
            return Body::from(inner_body);
        }
        // TODO
        panic!("TODO")
    }
}
