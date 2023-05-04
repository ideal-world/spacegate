use crate::{config::plugin_filter_dto::SgHttpPathModifier, functions::http_route::SgHttpRouteMatchInst};
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use tardis::url::Url;
use tardis::{
    basic::{error::TardisError, result::TardisResult},
    TardisFuns,
};

use super::{http_common_modify_path, SgPluginFilter, SgPluginFilterDef, SgRouteFilterContext};

pub const CODE: &str = "rewrite";

pub struct SgFilerRewriteDef;

impl SgPluginFilterDef for SgFilerRewriteDef {
    fn inst(&self, spec: serde_json::Value) -> TardisResult<Box<dyn SgPluginFilter>> {
        let filter = TardisFuns::json.json_to_obj::<SgFilerRewrite>(spec)?;
        Ok(Box::new(filter))
    }
}

/// RewriteFilter defines a filter that modifies a request during forwarding.
///
/// https://gateway-api.sigs.k8s.io/geps/gep-726/
#[derive(Default, Debug, Serialize, Deserialize, Clone)]
pub struct SgFilerRewrite {
    /// Hostname is the value to be used to replace the Host header value during forwarding.
    pub hostname: Option<String>,
    /// Path defines parameters used to modify the path of the incoming request. The modified path is then used to construct the Location header. When empty, the request path is used as-is.
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

    async fn req_filter(&self, _: &str, mut ctx: SgRouteFilterContext, matched_match_inst: Option<&SgHttpRouteMatchInst>) -> TardisResult<(bool, SgRouteFilterContext)> {
        if let Some(hostname) = &self.hostname {
            let mut uri = Url::parse(&ctx.get_req_uri().to_string())?;
            uri.set_host(Some(hostname)).map_err(|_| TardisError::format_error(&format!("[SG.Filter.Rewrite] Host {hostname} parsing error"), ""))?;
            ctx.set_req_uri(uri.as_str().parse().unwrap());
        }
        if let Some(new_url) = http_common_modify_path(ctx.get_req_uri(), &self.path, matched_match_inst)? {
            ctx.set_req_uri(new_url);
        }
        Ok((true, ctx))
    }

    async fn resp_filter(&self, _: &str, ctx: SgRouteFilterContext, _: Option<&SgHttpRouteMatchInst>) -> TardisResult<(bool, SgRouteFilterContext)> {
        Ok((true, ctx))
    }
}

#[cfg(test)]
mod tests {
    use crate::{
        config::{http_route_dto::SgHttpPathMatchType, plugin_filter_dto::SgHttpPathModifierType},
        functions::http_route::SgHttpPathMatchInst,
    };

    use super::*;
    use http::{HeaderMap, Method, StatusCode, Uri, Version};
    use hyper::Body;
    use tardis::tokio;

    #[tokio::test]
    async fn test_rewrite_filter() {
        let filter = SgFilerRewrite {
            hostname: Some("sg_new.idealworld.group".to_string()),
            path: Some(SgHttpPathModifier {
                kind: SgHttpPathModifierType::ReplacePrefixMatch,
                value: "/new_iam".to_string(),
            }),
        };

        let ctx = SgRouteFilterContext::new(
            Method::POST,
            Uri::from_static("http://sg.idealworld.group/iam/ct/001?name=sg"),
            Version::HTTP_11,
            HeaderMap::new(),
            Body::empty(),
            "127.0.0.1:8080".parse().unwrap(),
            "".to_string(),
        );
        let matched = SgHttpRouteMatchInst {
            path: Some(SgHttpPathMatchInst {
                kind: SgHttpPathMatchType::Prefix,
                value: "/iam".to_string(),
                regular: None,
            }),
            ..Default::default()
        };

        let (is_continue, mut ctx) = filter.req_filter("", ctx, Some(&matched)).await.unwrap();
        assert!(is_continue);
        assert_eq!(ctx.get_req_uri().to_string(), "http://sg_new.idealworld.group/new_iam/ct/001?name=sg");
        assert_eq!(ctx.get_resp_status_code(), &StatusCode::OK);

        let (is_continue, mut ctx) = filter.resp_filter("", ctx, Some(&matched)).await.unwrap();
        assert!(is_continue);
        assert_eq!(ctx.get_resp_status_code(), &StatusCode::OK);
    }
}
