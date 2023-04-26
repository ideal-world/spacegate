use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use tardis::{basic::result::TardisResult, TardisFuns};
use url::Url;

use crate::{
    config::plugin_filter_dto::{SgHttpPathModifier, SgHttpPathModifierType},
    functions::http_route::SgHttpRouteMatchInst,
};

use super::{SgPluginFilter, SgPluginFilterDef, SgRouteFilterContext, modify_path};

pub const CODE: &str = "rewrite";

pub struct SgFilerRewriteDef;

impl SgPluginFilterDef for SgFilerRewriteDef {
    fn new(&self, spec: serde_json::Value) -> TardisResult<Box<dyn SgPluginFilter>> {
        let filter = TardisFuns::json.json_to_obj::<SgFilerRewrite>(spec)?;
        Ok(Box::new(filter))
    }
}

/// RewriteFilter defines a filter that modifies a request during forwarding.
#[derive(Default, Debug, Serialize, Deserialize, Clone)]
pub struct SgFilerRewrite {
    /// Hostname is the value to be used to replace the Host header value during forwarding.
    pub hostname: Option<String>,
    pub path: Option<SgHttpPathModifier>,
}

#[async_trait]
impl SgPluginFilter for SgFilerRewrite {
    fn kind(&self) -> super::SgPluginFilterKind {
        super::SgPluginFilterKind::Http
    }

    async fn init(&self) -> TardisResult<()> {
        Ok(())
    }

    async fn destroy(&self) -> TardisResult<()> {
        Ok(())
    }

    async fn req_filter(&self, mut ctx: SgRouteFilterContext, matched_match_inst: Option<&SgHttpRouteMatchInst>) -> TardisResult<(bool, SgRouteFilterContext)> {
        if let Some(hostname) = &self.hostname {
            ctx.set_req_header("Host", hostname)?;
        }
        let ctx: SgRouteFilterContext = modify_path(&self.path, ctx,matched_match_inst)?;
        Ok((true, ctx))
    }

    async fn resp_filter(&self, ctx: SgRouteFilterContext, _: Option<&SgHttpRouteMatchInst>) -> TardisResult<(bool, SgRouteFilterContext)> {
        Ok((true, ctx))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use http::{HeaderMap, StatusCode, Version};
    use hyper::Body;
    use std::collections::HashMap;
    use tardis::tokio;

    #[tokio::test]
    async fn test_redirect_filter() {}
}
