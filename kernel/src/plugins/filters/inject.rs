use async_trait::async_trait;
use http::{HeaderName, Method};
use serde::{Deserialize, Serialize};
use tardis::{
    basic::{error::TardisError, result::TardisResult},
    TardisFuns,
};

use crate::functions::http_client;

use super::{BoxSgPluginFilter, SgPluginFilter, SgPluginFilterDef, SgPluginFilterInitDto, SgRoutePluginContext};

pub const CODE: &str = "inject";

pub struct SgFilterInjectDef;

impl SgPluginFilterDef for SgFilterInjectDef {
    fn inst(&self, spec: serde_json::Value) -> TardisResult<BoxSgPluginFilter> {
        let filter = TardisFuns::json.json_to_obj::<SgFilterInject>(spec)?;
        Ok(filter.boxed())
    }
}

#[derive(Default, Debug, Serialize, Deserialize, Clone)]
pub struct SgFilterInject {
    pub req_inject_url: Option<String>,
    pub req_timeout_ms: Option<u64>,
    pub resp_inject_url: Option<String>,
    pub resp_timeout_ms: Option<u64>,
}

const SG_INJECT_REAL_METHOD: HeaderName = HeaderName::from_static("sg-inject-real-method");
const SG_INJECT_REAL_URL: HeaderName = HeaderName::from_static("sg-inject-real-url");

#[async_trait]
impl SgPluginFilter for SgFilterInject {
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
        if let Some(req_inject_url) = &self.req_inject_url {
            let real_method = ctx.request.get_method().clone();
            let real_url = ctx.request.get_uri().clone();
            ctx.request.set_header(SG_INJECT_REAL_METHOD, real_method.as_str())?;
            ctx.request.set_header(SG_INJECT_REAL_URL, &real_url.to_string())?;
            let mut resp = http_client::raw_request(
                None,
                Method::PUT,
                req_inject_url,
                ctx.request.pop_body_raw()?,
                ctx.request.get_headers(),
                self.req_timeout_ms,
            )
            .await?;
            let new_req_headers = resp.headers_mut();
            let new_req_method = new_req_headers
                .get(SG_INJECT_REAL_METHOD)
                .map(|m| {
                    Method::from_bytes(m.to_str().map_err(|e| TardisError::bad_request(&format!("[SG.Filter.Inject] parse method error:{}", e), ""))?.as_bytes())
                        .map_err(|e| TardisError::bad_request(&format!("[SG.Filter.Inject] parse method error:{}", e), ""))
                })
                .transpose()?
                .unwrap_or(real_method);
            let new_req_url = new_req_headers
                .get(SG_INJECT_REAL_URL)
                .map(|m| {
                    m.to_str()
                        .map_err(|e| TardisError::bad_request(&format!("[SG.Filter.Inject] parse url error:{}", e), ""))?
                        .parse()
                        .map_err(|e| TardisError::bad_request(&format!("[SG.Filter.Inject] parse url error:{}", e), ""))
                })
                .transpose()?
                .unwrap_or(real_url);
            new_req_headers.remove(SG_INJECT_REAL_METHOD);
            new_req_headers.remove(SG_INJECT_REAL_URL);
            ctx = SgRoutePluginContext::new_http(
                new_req_method,
                new_req_url,
                *ctx.request.get_version(),
                new_req_headers.clone(),
                resp.into_body(),
                *ctx.request.get_remote_addr(),
                ctx.get_gateway_name(),
                None,
            )
        }
        Ok((true, ctx))
    }

    async fn resp_filter(&self, _: &str, mut ctx: SgRoutePluginContext) -> TardisResult<(bool, SgRoutePluginContext)> {
        if let Some(resp_inject_url) = &self.resp_inject_url {
            let real_method = ctx.request.get_method().clone();
            let real_url = ctx.request.get_uri().clone();
            ctx.response.set_header(SG_INJECT_REAL_METHOD, real_method.as_str())?;
            ctx.response.set_header(SG_INJECT_REAL_URL, &real_url.to_string())?;
            let resp = http_client::raw_request(
                None,
                Method::PUT,
                resp_inject_url,
                ctx.response.pop_body_raw()?,
                ctx.response.get_headers(),
                self.resp_timeout_ms,
            )
            .await?;
            ctx = ctx.resp(resp.status(), resp.headers().clone(), resp.into_body());
        }
        Ok((true, ctx))
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {

    use super::*;
    use http::{HeaderMap, StatusCode, Uri, Version};
    use hyper::Body;
    use tardis::tokio;

    #[tokio::test]
    async fn test_inject_filter() {
        http_client::init().unwrap();

        let filter = SgFilterInject {
            req_inject_url: Some("https://postman-echo.com/put".to_string()),
            resp_inject_url: Some("https://postman-echo.com/put".to_string()),
            ..Default::default()
        };

        let ctx = SgRoutePluginContext::new_http(
            Method::POST,
            Uri::from_static("http://sg.idealworld.group/iam/ct/001?name=sg"),
            Version::HTTP_11,
            HeaderMap::new(),
            Body::from("理想世界".as_bytes()),
            "127.0.0.1:8080".parse().unwrap(),
            "".to_string(),
            None,
        );

        let (is_continue, mut ctx) = filter.req_filter("", ctx).await.unwrap();
        assert!(is_continue);
        assert_eq!(ctx.request.get_uri().to_string(), "http://sg.idealworld.group/iam/ct/001?name=sg");
        let body = String::from_utf8(ctx.request.pop_body().await.unwrap().unwrap()).unwrap();
        assert!(body.contains(r#""url": "https://postman-echo.com/put""#));
        assert!(body.contains(r#""data": "理想世界""#));

        ctx.response.set_body("idealworld".as_bytes().to_vec()).unwrap();
        let (is_continue, mut ctx) = filter.resp_filter("", ctx).await.unwrap();
        assert!(is_continue);
        assert_eq!(ctx.response.get_status_code(), &StatusCode::OK);
        assert_eq!(ctx.request.get_uri().to_string(), "http://sg.idealworld.group/iam/ct/001?name=sg");
        let body = String::from_utf8(ctx.response.pop_body().await.unwrap().unwrap()).unwrap();
        assert!(body.contains(r#""url": "https://postman-echo.com/put""#));
        assert!(body.contains(r#""data": "idealworld""#));
    }
}
