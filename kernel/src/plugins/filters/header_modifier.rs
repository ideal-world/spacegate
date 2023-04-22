use std::collections::HashMap;

use super::{SgPluginFilter, SgRouteFilterContext};
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use tardis::basic::{error::TardisError, result::TardisResult};

#[derive(Default, Debug, Serialize, Deserialize, Clone)]
pub struct SgFilerHeaderModifier {
    kind: SgFilerHeaderModifierKind,
    sets: Option<HashMap<String, String>>,
    remove: Option<Vec<String>>,
}

#[derive(Debug, Serialize, Deserialize, PartialEq, Eq, Clone)]
pub enum SgFilerHeaderModifierKind {
    Request,
    Response,
}

impl Default for SgFilerHeaderModifierKind {
    fn default() -> Self {
        SgFilerHeaderModifierKind::Request
    }
}

#[async_trait]
impl SgPluginFilter for SgFilerHeaderModifier {
    fn kind(&self) -> super::SgPluginFilterKind {
        super::SgPluginFilterKind::Http
    }

    async fn req_filter(&self, mut ctx: SgRouteFilterContext) -> TardisResult<(bool, SgRouteFilterContext)> {
        if self.kind != SgFilerHeaderModifierKind::Request {
            return Ok((true, ctx));
        }
        if let Some(set) = &self.sets {
            for (k, v) in set.iter() {
                let name = http::header::HeaderName::try_from(k)
                    .map_err(|error| TardisError::format_error(&format!("[SG.filter.header_modifier] Header name {k} parsing error: {error} "), ""))?;
                let value = http::header::HeaderValue::try_from(v)
                    .map_err(|error| TardisError::format_error(&format!("[SG.filter.header_modifier] Header value {v} parsing error: {error} "), ""))?;
                ctx.req_headers.insert(name, value);
            }
        }
        if let Some(remove) = &self.remove {
            for k in remove {
                ctx.req_headers.remove(k);
            }
        }
        Ok((true, ctx))
    }

    async fn resp_filter(&self, mut ctx: SgRouteFilterContext) -> TardisResult<(bool, SgRouteFilterContext)> {
        if self.kind != SgFilerHeaderModifierKind::Response {
            return Ok((true, ctx));
        }
        if let Some(resp_headers) = ctx.resp_headers.as_mut() {
            if let Some(set) = &self.sets {
                for (k, v) in set.iter() {
                    let name = http::header::HeaderName::try_from(k)
                        .map_err(|error| TardisError::format_error(&format!("[SG.filter.header_modifier] Header name {k} parsing error: {error} "), ""))?;
                    let value = http::header::HeaderValue::try_from(v)
                        .map_err(|error| TardisError::format_error(&format!("[SG.filter.header_modifier] Header value {v} parsing error: {error} "), ""))?;
                    resp_headers.insert(name, value);
                }
            }
            if let Some(remove) = &self.remove {
                for k in remove {
                    resp_headers.remove(k);
                }
            }
        }
        Ok((true, ctx))
    }
}
