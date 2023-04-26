use async_trait::async_trait;
use http::{Method, StatusCode, Uri};
use serde::{Deserialize, Serialize};
use tardis::{
    basic::{error::TardisError, result::TardisResult},
    TardisFuns,
};
use url::Url;

use crate::{config::plugin_filter_dto::SgHttpPathModifier, functions::http_route::SgHttpRouteMatchInst};

use super::{modify_path, SgPluginFilter, SgPluginFilterDef, SgRouteFilterContext, SgRouteFilterRequestAction};

pub const CODE: &str = "redirect";

pub struct SgFilerRedirectDef;

impl SgPluginFilterDef for SgFilerRedirectDef {
    fn new(&self, spec: serde_json::Value) -> TardisResult<Box<dyn SgPluginFilter>> {
        let filter = TardisFuns::json.json_to_obj::<SgFilerRedirect>(spec)?;
        Ok(Box::new(filter))
    }
}

/// RedirectFilter defines a filter that redirects a request.
#[derive(Default, Debug, Serialize, Deserialize, Clone)]
pub struct SgFilerRedirect {
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

    async fn req_filter(&self, mut ctx: SgRouteFilterContext, matched_match_inst: Option<&SgHttpRouteMatchInst>) -> TardisResult<(bool, SgRouteFilterContext)> {
        if let Some(hostname) = &self.hostname {
            ctx.set_req_header("Host", hostname)?;
        }
        if let Some(scheme) = &self.scheme {
            let mut uri = Url::parse(&ctx.get_req_uri().to_string())?;
            uri.set_scheme(scheme).map_err(|_| TardisError::format_error(&format!("[SG.Filter.Redirect] Scheme {scheme} parsing error"), ""))?;
            ctx.set_req_uri(uri.as_str().parse().unwrap());
        }
        if let Some(port) = self.port {
            let mut uri = Url::parse(&ctx.get_req_uri().to_string())?;
            uri.set_port(Some(port)).map_err(|_| TardisError::format_error(&format!("[SG.Filter.Redirect] Port {port} parsing error"), ""))?;
            ctx.set_req_uri(uri.as_str().parse().unwrap());
        }
        let mut ctx = modify_path(&self.path, ctx, matched_match_inst)?;
        ctx.action = SgRouteFilterRequestAction::Redirect;
        Ok((true, ctx))
    }

    async fn resp_filter(&self, mut ctx: SgRouteFilterContext, _: Option<&SgHttpRouteMatchInst>) -> TardisResult<(bool, SgRouteFilterContext)> {
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
mod tests {
    use super::*;
    use http::{HeaderMap, StatusCode, Version};
    use hyper::Body;
    use std::collections::HashMap;
    use tardis::tokio;

    #[tokio::test]
    async fn test_redirect_filter() {
        // let mut headers = HashMap::new();
        // headers.insert("X-Test".to_string(), "test".to_string());
        // let filter = SgFilerRedirect {
        //     method: Some("post".to_string()),
        //     headers: Some(headers),
        //     uri: Some("http://httpbin.org/anything".to_string()),
        //     status_code: Some(StatusCode::MOVED_PERMANENTLY.as_u16()),
        // };

        // let ctx = SgRouteFilterContext::new(
        //     Method::GET,
        //     Uri::from_static("http://sg.idealworld.group/spi/cache/1"),
        //     Version::HTTP_11,
        //     HeaderMap::new(),
        //     Body::empty(),
        //     "127.0.0.1:8080".parse().unwrap(),
        //     "".to_string(),
        // );

        // let (is_continue, mut ctx) = filter.req_filter(ctx).await.unwrap();
        // assert!(is_continue);
        // assert_eq!(ctx.get_req_method().as_str().to_lowercase(), Method::POST.as_str().to_lowercase());
        // assert_eq!(ctx.get_req_headers().len(), 1);
        // assert_eq!(ctx.get_req_headers().get("X-Test").as_ref().unwrap().to_str().unwrap(), "test");
        // assert_eq!(ctx.get_req_uri().host().unwrap(), "httpbin.org");
        // assert_eq!(ctx.get_resp_status_code(), &StatusCode::OK);

        // let (is_continue, mut ctx) = filter.resp_filter(ctx).await.unwrap();
        // assert!(is_continue);
        // assert_eq!(ctx.get_resp_status_code(), &StatusCode::MOVED_PERMANENTLY);
    }
}
