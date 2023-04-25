use std::collections::HashMap;

use async_trait::async_trait;
use http::{Method, StatusCode, Uri};
use serde::{Deserialize, Serialize};
use tardis::{
    basic::{error::TardisError, result::TardisResult},
    TardisFuns,
};

use super::{SgPluginFilter, SgPluginFilterDef, SgRouteFilterContext, SgRouteFilterRequestAction};

pub const CODE: &str = "redirect";

pub struct SgFilerRedirectDef;

impl SgPluginFilterDef for SgFilerRedirectDef {
    fn new(&self, spec: serde_json::Value) -> TardisResult<Box<dyn SgPluginFilter>> {
        let filter = TardisFuns::json.json_to_obj::<SgFilerRedirect>(spec)?;
        Ok(Box::new(filter))
    }
}

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

    async fn init(&self) -> TardisResult<()> {
        Ok(())
    }

    async fn destroy(&self) -> TardisResult<()> {
        Ok(())
    }

    async fn req_filter(&self, mut ctx: SgRouteFilterContext) -> TardisResult<(bool, SgRouteFilterContext)> {
        if let Some(method) = self.method.as_ref() {
            ctx.set_req_method(
                Method::from_bytes(method.as_bytes())
                    .map_err(|error| TardisError::format_error(&format!("[SG.Filter.Redirect] Method name {method} parsing error: {error} "), ""))?,
            );
        }
        if let Some(headers) = self.headers.as_ref() {
            for (k, v) in headers.iter() {
                ctx.set_req_header(k, v)?;
            }
        }
        if let Some(uri) = self.uri.as_ref() {
            ctx.set_req_uri(Uri::try_from(uri).map_err(|error| TardisError::format_error(&format!("[SG.Filter.Redirect] Uri {uri} parsing error: {error} "), ""))?);
        }
        ctx.action = SgRouteFilterRequestAction::Redirect;
        Ok((true, ctx))
    }

    async fn resp_filter(&self, mut ctx: SgRouteFilterContext) -> TardisResult<(bool, SgRouteFilterContext)> {
        if let Some(status_code) = self.status_code {
            ctx.set_resp_status_code(
                StatusCode::from_u16(status_code)
                    .map_err(|error| TardisError::format_error(&format!("[SG.Filter.Redirect] Status code {status_code} parsing error: {error} "), ""))?,
            );
        }
        Ok((true, ctx))
    }
}
