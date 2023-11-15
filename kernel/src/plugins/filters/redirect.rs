use async_trait::async_trait;
use http::StatusCode;
use kernel_common::gatewayapi_support_filter::SgFilterRedirect;

use tardis::basic::{error::TardisError, result::TardisResult};
use tardis::url::Url;

use crate::def_filter;
use crate::helpers::url_helper::UrlToUri;
use crate::plugins::context::SgRouteFilterRequestAction;

use super::{http_common_modify_path, SgPluginFilter, SgPluginFilterInitDto, SgRoutePluginContext};

def_filter!(SG_FILTER_REDIRECT_CODE, SgFilterRedirectDef, SgFilterRedirect);

#[async_trait]
impl SgPluginFilter for SgFilterRedirect {
    fn accept(&self) -> super::SgPluginFilterAccept {
        super::SgPluginFilterAccept {
            kind: vec![super::SgPluginFilterKind::Http],
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
            uri.set_host(Some(hostname)).map_err(|_| TardisError::format_error(&format!("[SG.Filter.Redirect] Host {hostname} parsing error"), ""))?;
            ctx.request.set_uri(uri.to_uri()?);
        }
        if let Some(scheme) = &self.scheme {
            let mut uri = Url::parse(&ctx.request.get_uri().to_string())?;
            uri.set_scheme(scheme).map_err(|_| TardisError::format_error(&format!("[SG.Filter.Redirect] Scheme {scheme} parsing error"), ""))?;
            ctx.request.set_uri(uri.to_uri()?);
        }
        if let Some(port) = self.port {
            let mut uri = Url::parse(&ctx.request.get_uri().to_string())?;
            uri.set_port(Some(port)).map_err(|_| TardisError::format_error(&format!("[SG.Filter.Redirect] Port {port} parsing error"), ""))?;
            ctx.request.set_uri(uri.to_uri()?);
        }
        let matched_match_inst = ctx.get_rule_matched();
        if let Some(new_url) = http_common_modify_path(ctx.request.get_uri(), &self.path, matched_match_inst.as_ref())? {
            ctx.request.set_uri(new_url);
        }
        ctx.set_action(SgRouteFilterRequestAction::Redirect);
        Ok((true, ctx))
    }

    async fn resp_filter(&self, _: &str, mut ctx: SgRoutePluginContext) -> TardisResult<(bool, SgRoutePluginContext)> {
        if let Some(status_code) = self.status_code {
            ctx.response.set_status_code(
                StatusCode::from_u16(status_code)
                    .map_err(|error| TardisError::format_error(&format!("[SG.Filter.Redirect] Status code {status_code} parsing error: {error} "), ""))?,
            );
        }
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
    use kernel_common::inner_model::http_route::SgHttpPathMatchType;
    use kernel_common::inner_model::plugin_filter::{SgHttpPathModifier, SgHttpPathModifierType};
    use tardis::tokio;

    #[tokio::test]
    async fn test_redirect_filter() {
        let filter = SgFilterRedirect {
            scheme: Some("https".to_string()),
            hostname: Some("sg_new.idealworld.group".to_string()),
            path: Some(SgHttpPathModifier {
                kind: SgHttpPathModifierType::ReplacePrefixMatch,
                value: "/new_iam".to_string(),
            }),
            port: Some(443),
            status_code: Some(StatusCode::MOVED_PERMANENTLY.as_u16()),
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
        assert_eq!(ctx.request.get_uri().to_string(), "https://sg_new.idealworld.group/new_iam/ct/001?name=sg");
        assert_eq!(ctx.response.get_status_code(), &StatusCode::OK);

        let (is_continue, ctx) = filter.resp_filter("", ctx).await.unwrap();
        assert!(is_continue);
        assert_eq!(ctx.response.get_status_code(), &StatusCode::MOVED_PERMANENTLY);
    }
}
