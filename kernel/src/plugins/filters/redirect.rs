use async_trait::async_trait;
use http::StatusCode;
use serde::{Deserialize, Serialize};
use tardis::basic::{error::TardisError, result::TardisResult};
use tardis::url::Url;

use crate::def_filter;
use crate::helpers::url_helper::UrlToUri;
use crate::plugins::context::SgRouteFilterRequestAction;
use kernel_dto::dto::plugin_filter_dto::SgHttpPathModifier;

use super::{http_common_modify_path, SgPluginFilter, SgPluginFilterInitDto, SgRoutePluginContext};

def_filter!("redirect", SgFilterRedirectDef, SgFilterRedirect);

/// RedirectFilter defines a filter that redirects a request.
///
/// https://gateway-api.sigs.k8s.io/geps/gep-726/
#[derive(Default, Debug, Serialize, Deserialize, Clone)]
pub struct SgFilterRedirect {
    /// Scheme is the scheme to be used in the value of the Location header in the response. When empty, the scheme of the request is used.
    pub scheme: Option<String>,
    /// Hostname is the hostname to be used in the value of the Location header in the response. When empty, the hostname in the Host header of the request is used.
    pub hostname: Option<String>,
    /// Path defines parameters used to modify the path of the incoming request. The modified path is then used to construct the Location header. When empty, the request path is used as-is.
    pub path: Option<SgHttpPathModifier>,
    /// Port is the port to be used in the value of the Location header in the response.
    pub port: Option<u16>,
    /// StatusCode is the HTTP status code to be used in response.
    pub status_code: Option<u16>,
}

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
        config::{http_route_dto::SgHttpPathMatchType, plugin_filter_dto::SgHttpPathModifierType},
        instance::{SgHttpPathMatchInst, SgHttpRouteMatchInst, SgHttpRouteRuleInst},
        plugins::context::ChosenHttpRouteRuleInst,
    };

    use super::*;
    use http::{HeaderMap, Method, StatusCode, Uri, Version};
    use hyper::Body;
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
            Some(ChosenHttpRouteRuleInst::clone_from(&SgHttpRouteRuleInst::default(), Some(&matched))),
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
