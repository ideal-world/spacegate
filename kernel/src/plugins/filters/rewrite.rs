use crate::def_filter;
use crate::helpers::url_helper::UrlToUri;
use async_trait::async_trait;
use kernel_common::gatewayapi_support_filter::{SgFilterRewrite, SG_FILTER_REWRITE_CODE};
use tardis::basic::{error::TardisError, result::TardisResult};
use tardis::url::Url;

use super::{http_common_modify_path, SgPluginFilter, SgPluginFilterInitDto, SgRoutePluginContext};

def_filter!(SG_FILTER_REWRITE_CODE, SgFilterRewriteDef, SgFilterRewrite);

#[async_trait]
impl SgPluginFilter for SgFilterRewrite {
    fn accept(&self) -> super::SgPluginFilterAccept {
        super::SgPluginFilterAccept {
            kind: vec![super::SgPluginFilterKind::Http, super::SgPluginFilterKind::Ws],
            ..Default::default()
        }
    }

    async fn init(&mut self, _: &SgPluginFilterInitDto) -> TardisResult<()> {
        Ok(())
    }

    async fn destroy(&self) -> TardisResult<()> {
        Ok(())
    }

    async fn req_filter(&self, _: &str, mut ctx: SgRoutePluginContext) -> TardisResult<(bool, SgRoutePluginContext)> {
        if let Some(hostname) = &self.hostname {
            let mut uri = Url::parse(&ctx.request.get_uri().to_string())?;
            uri.set_host(Some(hostname)).map_err(|_| TardisError::format_error(&format!("[SG.Filter.Rewrite] Host {hostname} parsing error"), ""))?;
            ctx.request.set_uri(uri.to_uri()?);
        }
        let matched_match_inst = ctx.get_rule_matched();
        if let Some(new_url) = http_common_modify_path(ctx.request.get_uri(), &self.path, matched_match_inst.as_ref())? {
            ctx.request.set_uri(new_url);
        }
        Ok((true, ctx))
    }

    async fn resp_filter(&self, _: &str, ctx: SgRoutePluginContext) -> TardisResult<(bool, SgRoutePluginContext)> {
        Ok((true, ctx))
    }
}

#[cfg(test)]

mod tests {
    use crate::{
        instance::{SgHttpPathMatchInst, SgHttpRouteMatchInst, SgHttpRouteRuleInst},
        plugins::context::ChosenHttpRouteRuleInst,
    };

    use super::*;
    use http::{HeaderMap, Method, StatusCode, Uri, Version};
    use hyper::Body;
    use kernel_common::inner_model::plugin_filter::SgHttpPathModifierType;
    use kernel_common::inner_model::{http_route::SgHttpPathMatchType, plugin_filter::SgHttpPathModifier};
    use tardis::tokio;

    #[tokio::test]
    async fn test_rewrite_filter() {
        let filter = SgFilterRewrite {
            hostname: Some("sg_new.idealworld.group".to_string()),
            path: Some(SgHttpPathModifier {
                kind: SgHttpPathModifierType::ReplacePrefixMatch,
                value: "/new_iam".to_string(),
            }),
        };

        let matched = SgHttpRouteMatchInst {
            path: Some(SgHttpPathMatchInst {
                kind: SgHttpPathMatchType::Prefix,
                value: "/iam".to_string(),
                regular: None,
            }),
            ..Default::default()
        };

        let ctx = SgRoutePluginContext::new_http(
            Method::POST,
            Uri::from_static("http://sg.idealworld.group/iam/ct/001?name=sg"),
            Version::HTTP_11,
            HeaderMap::new(),
            Body::empty(),
            "127.0.0.1:8080".parse().unwrap(),
            "".to_string(),
            Some(ChosenHttpRouteRuleInst::cloned_from(&SgHttpRouteRuleInst::default(), Some(&matched))),
            None,
        );

        let (is_continue, ctx) = filter.req_filter("", ctx).await.unwrap();
        assert!(is_continue);
        assert_eq!(ctx.request.get_uri().to_string(), "http://sg_new.idealworld.group/new_iam/ct/001?name=sg");
        assert_eq!(ctx.response.get_status_code(), &StatusCode::OK);

        let (is_continue, ctx) = filter.resp_filter("", ctx).await.unwrap();
        assert!(is_continue);
        assert_eq!(ctx.response.get_status_code(), &StatusCode::OK);
    }
}
