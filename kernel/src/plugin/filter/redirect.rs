use std::collections::HashMap;

use async_trait::async_trait;
use http::{Method, StatusCode, Uri};
use serde::{Deserialize, Serialize};
use tardis::basic::{error::TardisError, result::TardisResult};

use super::{SgPluginFilter, SgRouteFilterContext};

#[derive(Default, Debug, Serialize, Deserialize, Clone)]
pub struct SgFilerRedirect {
    pub method: Option<String>,
    pub headers: Option<HashMap<String, String>>,
    pub uri: Option<String>,
    pub status_code: Option<u16>,
}

#[async_trait]
impl SgPluginFilter for SgFilerRedirect {
    fn kind(&self) -> super::SgPluginFilterKind {
        super::SgPluginFilterKind::Http
    }

    async fn req_filter(&self, mut ctx: SgRouteFilterContext) -> TardisResult<(bool, SgRouteFilterContext)> {
        if let Some(method) = self.method.as_ref() {
            ctx.method = Method::from_bytes(method.as_bytes())
                .map_err(|error| TardisError::format_error(&format!("[SG.filter.redirect] Method name {method} parsing error: {error} "), ""))?;
        }
        if let Some(headers) = self.headers.as_ref() {
            for (k, v) in headers.iter() {
                let name = http::header::HeaderName::try_from(k)
                    .map_err(|error| TardisError::format_error(&format!("[SG.filter.redirect] Header name {k} parsing error: {error} "), ""))?;
                let value = http::header::HeaderValue::try_from(v)
                    .map_err(|error| TardisError::format_error(&format!("[SG.filter.redirect] Header value {v} parsing error: {error} "), ""))?;
                ctx.req_headers.insert(name, value);
            }
        }
        if let Some(uri) = self.uri.as_ref() {
            ctx.uri = Uri::try_from(uri).map_err(|error| TardisError::format_error(&format!("[SG.filter.redirect] Uri {uri} parsing error: {error} "), ""))?;
        }
        Ok((true, ctx))
    }

    async fn resp_filter(&self, mut ctx: SgRouteFilterContext) -> TardisResult<(bool, SgRouteFilterContext)> {
        if let Some(status_code) = self.status_code {
            ctx.resp_status_code = Some(
                StatusCode::from_u16(status_code)
                    .map_err(|error| TardisError::format_error(&format!("[SG.filter.redirect] Status code {status_code} parsing error: {error} "), ""))?,
            );
        }
        Ok((true, ctx))
    }
}
