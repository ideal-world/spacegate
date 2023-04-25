use std::collections::HashMap;

use super::{SgPluginFilter, SgPluginFilterDef, SgRouteFilterContext};
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use tardis::{basic::result::TardisResult, TardisFuns};

pub const CODE: &str = "header_modifier";

pub struct SgFilerHeaderModifierDef;

impl SgPluginFilterDef for SgFilerHeaderModifierDef {
    fn new(&self, spec: serde_json::Value) -> TardisResult<Box<dyn SgPluginFilter>> {
        let filter = TardisFuns::json.json_to_obj::<SgFilerHeaderModifier>(spec)?;
        Ok(Box::new(filter))
    }
}

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

    async fn init(&self) -> TardisResult<()> {
        Ok(())
    }

    async fn destroy(&self) -> TardisResult<()> {
        Ok(())
    }

    async fn req_filter(&self, mut ctx: SgRouteFilterContext) -> TardisResult<(bool, SgRouteFilterContext)> {
        if self.kind != SgFilerHeaderModifierKind::Request {
            return Ok((true, ctx));
        }
        if let Some(set) = &self.sets {
            for (k, v) in set.iter() {
                ctx.set_req_header(k, v)?;
            }
        }
        if let Some(remove) = &self.remove {
            for k in remove {
                ctx.remove_req_header(k)?;
            }
        }
        Ok((true, ctx))
    }

    async fn resp_filter(&self, mut ctx: SgRouteFilterContext) -> TardisResult<(bool, SgRouteFilterContext)> {
        if self.kind != SgFilerHeaderModifierKind::Response {
            return Ok((true, ctx));
        }
        if let Some(set) = &self.sets {
            for (k, v) in set.iter() {
                ctx.set_resp_header(k, v)?;
            }
        }
        if let Some(remove) = &self.remove {
            for k in remove {
                ctx.remove_resp_header(k)?;
            }
        }
        Ok((true, ctx))
    }
}
