use async_trait::async_trait;
use http::StatusCode;
use serde::{Deserialize, Serialize};
use tardis::url::Url;
use tardis::{
    basic::{error::TardisError, result::TardisResult},
    TardisFuns,
};

use crate::config::http_route_dto::SgHttpRouteRule;
use crate::helpers::url_helper::UrlToUri;
use crate::plugins::context::SgRouteFilterRequestAction;
use crate::{config::plugin_filter_dto::SgHttpPathModifier, functions::http_route::SgHttpRouteMatchInst};

use super::{http_common_modify_path, BoxSgPluginFilter, SgPluginFilter, SgPluginFilterDef, SgRoutePluginContext};

pub const CODE: &str = "redirect";

pub struct SgFilterRedirectDef;

impl SgPluginFilterDef for SgFilterRedirectDef {
    fn inst(&self, spec: serde_json::Value) -> TardisResult<BoxSgPluginFilter> {
        let filter = TardisFuns::json.json_to_obj::<SgFilterRedirect>(spec)?;
        Ok(filter.boxed())
    }
}

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
    async fn init(&self, _: &[SgHttpRouteRule]) -> TardisResult<()> {
        Ok(())
    }

    async fn destroy(&self) -> TardisResult<()> {
        Ok(())
    }

    async fn req_filter(&self, _: &str, mut ctx: SgRoutePluginContext, matched_match_inst: Option<&SgHttpRouteMatchInst>) -> TardisResult<(bool, SgRoutePluginContext)> {
        if let Some(hostname) = &self.hostname {
            let mut uri = Url::parse(&ctx.get_req_uri().to_string())?;
            uri.set_host(Some(hostname)).map_err(|_| TardisError::format_error(&format!("[SG.Filter.Redirect] Host {hostname} parsing error"), ""))?;
            ctx.set_req_uri(uri.to_uri()?);
        }
        if let Some(scheme) = &self.scheme {
            let mut uri = Url::parse(&ctx.get_req_uri().to_string())?;
            uri.set_scheme(scheme).map_err(|_| TardisError::format_error(&format!("[SG.Filter.Redirect] Scheme {scheme} parsing error"), ""))?;
            ctx.set_req_uri(uri.to_uri()?);
        }
        if let Some(port) = self.port {
            let mut uri = Url::parse(&ctx.get_req_uri().to_string())?;
            uri.set_port(Some(port)).map_err(|_| TardisError::format_error(&format!("[SG.Filter.Redirect] Port {port} parsing error"), ""))?;
            ctx.set_req_uri(uri.to_uri()?);
        }
        if let Some(new_url) = http_common_modify_path(ctx.get_req_uri(), &self.path, matched_match_inst)? {
            ctx.set_req_uri(new_url);
        }
        ctx.set_action(SgRouteFilterRequestAction::Redirect);
        Ok((true, ctx))
    }

    async fn resp_filter(&self, _: &str, mut ctx: SgRoutePluginContext, _: Option<&SgHttpRouteMatchInst>) -> TardisResult<(bool, SgRoutePluginContext)> {
        if let Some(status_code) = self.status_code {
            ctx.set_resp_status_code(
                StatusCode::from_u16(status_code)
                    .map_err(|error| TardisError::format_error(&format!("[SG.Filter.Redirect] Status code {status_code} parsing error: {error} "), ""))?,
            );
        }
        Ok((true, ctx))
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use crate::{
        config::{http_route_dto::SgHttpPathMatchType, plugin_filter_dto::SgHttpPathModifierType},
        functions::http_route::{SgHttpPathMatchInst, SgHttpRouteRuleInst},
        plugins::context::ChoseHttpRouteRuleInst,
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
            Some(ChoseHttpRouteRuleInst::clone_from(&SgHttpRouteRuleInst::default(), Some(&matched))),
        );

        let (is_continue, mut ctx) = filter.req_filter("", ctx, Some(&matched)).await.unwrap();
        assert!(is_continue);
        assert_eq!(ctx.get_req_uri().to_string(), "https://sg_new.idealworld.group/new_iam/ct/001?name=sg");
        assert_eq!(ctx.get_resp_status_code(), &StatusCode::OK);

        let (is_continue, mut ctx) = filter.resp_filter("", ctx, Some(&matched)).await.unwrap();
        assert!(is_continue);
        assert_eq!(ctx.get_resp_status_code(), &StatusCode::MOVED_PERMANENTLY);
    }
}
