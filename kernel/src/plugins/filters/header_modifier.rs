use std::collections::HashMap;

use super::{BoxSgPluginFilter, SgPluginFilter, SgPluginFilterDef, SgPluginFilterInitDto, SgRoutePluginContext};
use async_trait::async_trait;
use http::HeaderName;
use serde::{Deserialize, Serialize};
use tardis::{
    basic::{error::TardisError, result::TardisResult},
    TardisFuns,
};

pub const CODE: &str = "header_modifier";

pub struct SgFilterHeaderModifierDef;

impl SgPluginFilterDef for SgFilterHeaderModifierDef {
    fn get_code(&self) -> &'static str {
        CODE
    }
    fn inst(&self, spec: serde_json::Value) -> TardisResult<BoxSgPluginFilter> {
        let filter = TardisFuns::json.json_to_obj::<SgFilterHeaderModifier>(spec)?;
        Ok(filter.boxed())
    }
}

#[derive(Default, Debug, Serialize, Deserialize, Clone)]
pub struct SgFilterHeaderModifier {
    pub kind: SgFilterHeaderModifierKind,
    pub sets: Option<HashMap<String, String>>,
    pub remove: Option<Vec<String>>,
}

#[derive(Debug, Serialize, Deserialize, PartialEq, Eq, Clone, Default)]
pub enum SgFilterHeaderModifierKind {
    #[default]
    Request,
    Response,
}

#[async_trait]
impl SgPluginFilter for SgFilterHeaderModifier {
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
        if self.kind != SgFilterHeaderModifierKind::Request {
            return Ok((true, ctx));
        }
        if let Some(set) = &self.sets {
            for (k, v) in set.iter() {
                ctx.request.set_header_str(k, v)?;
            }
        }
        if let Some(remove) = &self.remove {
            for k in remove {
                ctx.request
                    .get_headers_mut()
                    .remove(HeaderName::try_from(k).map_err(|error| TardisError::format_error(&format!("[SG.Filter] Header key {k} parsing error: {error}"), ""))?);
            }
        }
        Ok((true, ctx))
    }

    async fn resp_filter(&self, _: &str, mut ctx: SgRoutePluginContext) -> TardisResult<(bool, SgRoutePluginContext)> {
        if self.kind != SgFilterHeaderModifierKind::Response {
            return Ok((true, ctx));
        }
        if let Some(set) = &self.sets {
            for (k, v) in set.iter() {
                ctx.response.set_header_str(k, v)?;
            }
        }
        if let Some(remove) = &self.remove {
            for k in remove {
                ctx.response.remove_header_str(k)?;
            }
        }
        Ok((true, ctx))
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {

    use super::*;
    use http::{HeaderMap, Method, StatusCode, Uri, Version};
    use hyper::Body;
    use std::collections::HashMap;
    use tardis::tokio;

    #[tokio::test]
    async fn test_header_modifier_filter() {
        let mut headers = HashMap::new();
        headers.insert("X-Test1".to_string(), "test1".to_string());
        headers.insert("X-Test2".to_string(), "test2".to_string());
        let filter_req = SgFilterHeaderModifier {
            kind: SgFilterHeaderModifierKind::Request,
            sets: Some(headers),
            remove: Some(vec!["X-1".to_string(), "X-2".to_string()]),
        };
        let filter_resp = SgFilterHeaderModifier {
            kind: SgFilterHeaderModifierKind::Response,
            sets: None,
            remove: Some(vec!["X-Test2".to_string(), "X-2".to_string()]),
        };

        let mut req_headers = HeaderMap::new();
        req_headers.insert("X-Test1", "Hi".parse().unwrap());
        req_headers.insert("X-1", "Hi".parse().unwrap());
        let ctx = SgRoutePluginContext::new_http(
            Method::GET,
            Uri::from_static("http://sg.idealworld.group/spi/cache/1"),
            Version::HTTP_11,
            HeaderMap::new(),
            Body::empty(),
            "127.0.0.1:8080".parse().unwrap(),
            "".to_string(),
            None,
        );

        let (is_continue, mut ctx) = filter_req.req_filter("", ctx).await.unwrap();
        assert!(is_continue);
        assert_eq!(ctx.request.get_method().as_str().to_lowercase(), Method::GET.as_str().to_lowercase());
        assert_eq!(ctx.request.get_headers().len(), 2);
        assert_eq!(ctx.request.get_headers().get("X-Test1").as_ref().unwrap().to_str().unwrap(), "test1");
        assert_eq!(ctx.request.get_headers().get("X-Test2").as_ref().unwrap().to_str().unwrap(), "test2");
        assert_eq!(ctx.request.get_uri().host().unwrap(), "sg.idealworld.group");
        assert_eq!(ctx.response.get_status_code(), &StatusCode::OK);

        let mock_resp_headers = ctx.request.get_headers().clone();
        ctx.response.set_headers(mock_resp_headers);
        let (is_continue, mut ctx) = filter_resp.resp_filter("", ctx).await.unwrap();
        assert!(is_continue);
        assert_eq!(ctx.request.get_method().as_str().to_lowercase(), Method::GET.as_str().to_lowercase());
        assert_eq!(ctx.request.get_headers().len(), 2);
        assert_eq!(ctx.request.get_headers().get("X-Test1").as_ref().unwrap().to_str().unwrap(), "test1");
        assert_eq!(ctx.request.get_headers().get("X-Test2").as_ref().unwrap().to_str().unwrap(), "test2");
        assert_eq!(ctx.request.get_uri().host().unwrap(), "sg.idealworld.group");
        assert_eq!(ctx.response.get_headers().len(), 1);
        assert_eq!(ctx.response.get_headers().get("X-Test1").as_ref().unwrap().to_str().unwrap(), "test1");
        assert_eq!(ctx.response.get_status_code(), &StatusCode::OK);
    }
}
