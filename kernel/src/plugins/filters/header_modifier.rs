use std::collections::HashMap;

use crate::functions::http_route::SgHttpRouteMatchInst;

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

#[derive(Debug, Serialize, Deserialize, PartialEq, Eq, Clone, Default)]
pub enum SgFilerHeaderModifierKind {
    #[default]
    Request,
    Response,
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

    async fn req_filter(&self, _: &str, mut ctx: SgRouteFilterContext, _: Option<&SgHttpRouteMatchInst>) -> TardisResult<(bool, SgRouteFilterContext)> {
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

    async fn resp_filter(&self, _: &str, mut ctx: SgRouteFilterContext, _: Option<&SgHttpRouteMatchInst>) -> TardisResult<(bool, SgRouteFilterContext)> {
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

#[cfg(test)]
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
        let filter_req = SgFilerHeaderModifier {
            kind: SgFilerHeaderModifierKind::Request,
            sets: Some(headers),
            remove: Some(vec!["X-1".to_string(), "X-2".to_string()]),
        };
        let filter_resp = SgFilerHeaderModifier {
            kind: SgFilerHeaderModifierKind::Response,
            sets: None,
            remove: Some(vec!["X-Test2".to_string(), "X-2".to_string()]),
        };

        let mut req_headers = HeaderMap::new();
        req_headers.insert("X-Test1", "Hi".parse().unwrap());
        req_headers.insert("X-1", "Hi".parse().unwrap());
        let ctx = SgRouteFilterContext::new(
            Method::GET,
            Uri::from_static("http://sg.idealworld.group/spi/cache/1"),
            Version::HTTP_11,
            HeaderMap::new(),
            Body::empty(),
            "127.0.0.1:8080".parse().unwrap(),
            "".to_string(),
        );

        let (is_continue, mut ctx) = filter_req.req_filter("", ctx, None).await.unwrap();
        assert!(is_continue);
        assert_eq!(ctx.get_req_method().as_str().to_lowercase(), Method::GET.as_str().to_lowercase());
        assert_eq!(ctx.get_req_headers().len(), 2);
        assert_eq!(ctx.get_req_headers().get("X-Test1").as_ref().unwrap().to_str().unwrap(), "test1");
        assert_eq!(ctx.get_req_headers().get("X-Test2").as_ref().unwrap().to_str().unwrap(), "test2");
        assert_eq!(ctx.get_req_uri().host().unwrap(), "sg.idealworld.group");
        assert_eq!(ctx.get_resp_status_code(), &StatusCode::OK);

        let mock_resp_headers = ctx.get_req_headers().clone();
        ctx.set_resp_headers(mock_resp_headers);
        let (is_continue, mut ctx) = filter_resp.resp_filter("", ctx, None).await.unwrap();
        assert!(is_continue);
        assert_eq!(ctx.get_req_method().as_str().to_lowercase(), Method::GET.as_str().to_lowercase());
        assert_eq!(ctx.get_req_headers().len(), 2);
        assert_eq!(ctx.get_req_headers().get("X-Test1").as_ref().unwrap().to_str().unwrap(), "test1");
        assert_eq!(ctx.get_req_headers().get("X-Test2").as_ref().unwrap().to_str().unwrap(), "test2");
        assert_eq!(ctx.get_req_uri().host().unwrap(), "sg.idealworld.group");
        assert_eq!(ctx.get_resp_headers().len(), 1);
        assert_eq!(ctx.get_resp_headers().get("X-Test1").as_ref().unwrap().to_str().unwrap(), "test1");
        assert_eq!(ctx.get_resp_status_code(), &StatusCode::OK);
    }
}
