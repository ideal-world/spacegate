use async_compression::tokio::bufread::GzipEncoder;
use async_trait::async_trait;
use http::{header, HeaderValue};
use serde::{Deserialize, Serialize};
use tardis::{
    basic::result::TardisResult,
    tokio::io::{AsyncReadExt, BufReader},
    TardisFuns,
};

use crate::functions::http_route::SgRouteMatchInst;

use super::{BoxSgPluginFilter, SgPluginFilter, SgPluginFilterDef, SgRouteFilterContext};

pub const CODE: &str = "compression";
pub struct SgFilterCompressionDef;

impl SgPluginFilterDef for SgFilterCompressionDef {
    fn inst(&self, spec: serde_json::Value) -> TardisResult<BoxSgPluginFilter> {
        let filter = TardisFuns::json.json_to_obj::<SgFilterCompression>(spec)?;
        Ok(filter.boxed())
    }
}

///
#[derive(Default, Debug, Serialize, Deserialize, Clone)]
pub struct SgFilterCompression {}

#[derive(Default, Debug, Serialize, Deserialize, Clone, PartialEq)]
pub enum CompressionType {
    #[default]
    Gzip,
    Deflate,
    Br,
}

impl From<CompressionType> for HeaderValue {
    #[inline]
    fn from(algo: CompressionType) -> Self {
        HeaderValue::from_static(match algo {
            CompressionType::Gzip => "gzip",
            CompressionType::Deflate => "deflate",
            CompressionType::Br => "br",
        })
    }
}
impl From<CompressionType> for &str {
    #[inline]
    fn from(algo: CompressionType) -> Self {
        match algo {
            CompressionType::Gzip => "gzip",
            CompressionType::Deflate => "deflate",
            CompressionType::Br => "br",
        }
    }
}
#[async_trait]
impl SgPluginFilter for SgFilterCompression {
    fn kind(&self) -> super::SgPluginFilterKind {
        super::SgPluginFilterKind::Http
    }

    async fn init(&self) -> TardisResult<()> {
        Ok(())
    }

    async fn destroy(&self) -> TardisResult<()> {
        Ok(())
    }

    async fn req_filter(&self, _: &str, ctx: SgRouteFilterContext, _matched_match_inst: Option<&SgRouteMatchInst>) -> TardisResult<(bool, SgRouteFilterContext)> {
        Ok((true, ctx))
    }

    async fn resp_filter(&self, _: &str, mut ctx: SgRouteFilterContext, _: Option<&SgRouteMatchInst>) -> TardisResult<(bool, SgRouteFilterContext)> {
        let resp_body = ctx.pop_resp_body().await?;
        let req_headers = ctx.get_req_headers_raw();
        if let Some(resp_body) = resp_body {
            let resp_encode_type = get_encode_type(ctx.get_resp_headers_raw().get(header::CONTENT_ENCODING));
            let desired_response_encoding = get_encode_type(ctx.get_resp_headers_raw().get(header::ACCEPT_ENCODING));
            if desired_response_encoding == resp_encode_type {
                ctx.set_resp_body(resp_body)?;
                return Ok((true, ctx));
            }
            if let Some(desired_response_encoding) = desired_response_encoding {
                // if let Some(content_encoding_value) = ctx.get_resp_headers_raw().get(header::CONTENT_ENCODING) {
                //     if content_encoding_value.to_str().map_or(false, |v| v.contains::<&str>(CompressionType::Gzip.into())) {
                //         ctx.set_resp_body(resp_body)?;
                //         return Ok((true, ctx));
                //     }
                // }
                match desired_response_encoding {
                    CompressionType::Gzip => todo!(),
                    CompressionType::Deflate => todo!(),
                    CompressionType::Br => todo!(),
                }
                // if accept_encoding_value.to_str().map_or(false, |v| v.contains::<&str>(CompressionType::Gzip.into())) {
                //     ctx.set_resp_header(header::CONTENT_ENCODING.as_str(), CompressionType::Gzip.into())?;
                //     let mut gziped = GzipEncoder::new(BufReader::new(&resp_body[..]));
                //     let mut encoder_body = vec![];
                //     let _ = gziped.read_to_end(&mut encoder_body).await;

                //     ctx.set_resp_body(encoder_body)?;
                //     return Ok((true, ctx));
                // }
            }
            ctx.set_resp_body(resp_body)?;
        }
        Ok((true, ctx))
    }
}

fn get_encode_type(header_value: Option<&HeaderValue>) -> Option<CompressionType> {
    if let Some(header_value) = header_value {
        header_value.to_str().map_or_else(
            |_| None,
            |v_str| {
                if v_str.contains(";q=") {
                    let split: Vec<&str> = v_str.split(',').collect();
                    let result = None;
                    for s in split {
                        let split: Vec<&str> = v_str.split(";q=").collect();
                        if split.len() == 2 {
                            todo!()
                            //todo support ;q=
                        }
                        // if v_str.contains::<&str>(CompressionType::Gzip.into()) {
                        //     Some(CompressionType::Gzip)
                        // } else if v_str.contains::<&str>(CompressionType::Br.into()) {
                        //     Some(CompressionType::Br)
                        // } else if v_str.contains::<&str>(CompressionType::Deflate.into()) {
                        //     Some(CompressionType::Deflate)
                        // } else {
                        //     None
                        // }
                    }
                    result
                } else if v_str.contains::<&str>(CompressionType::Gzip.into()) {
                    Some(CompressionType::Gzip)
                } else if v_str.contains::<&str>(CompressionType::Br.into()) {
                    Some(CompressionType::Br)
                } else if v_str.contains::<&str>(CompressionType::Deflate.into()) {
                    Some(CompressionType::Deflate)
                } else {
                    None
                }
            },
        )
    } else {
        None
    }
}

#[cfg(test)]
mod tests {

    use super::*;
    use async_compression::tokio::bufread::GzipDecoder;
    use http::{HeaderMap, Method, StatusCode, Uri, Version};
    use hyper::Body;
    use tardis::tokio::{self};

    #[tokio::test]
    async fn test_gzip() {
        let filter = SgFilterCompression {};

        let mut header = HeaderMap::new();
        header.insert(header::ACCEPT_ENCODING, "gzip".parse().unwrap());
        let ctx = SgRouteFilterContext::new(
            Method::POST,
            Uri::from_static("http://sg.idealworld.group/"),
            Version::HTTP_11,
            header,
            Body::empty(),
            "127.0.0.1:8080".parse().unwrap(),
            "".to_string(),
        );
        let matched = SgRouteMatchInst { ..Default::default() };

        let (is_continue, mut ctx) = filter.req_filter("", ctx, Some(&matched)).await.unwrap();
        assert!(is_continue);
        let body_str = "test 1 测试 1 ";
        let resp_body = Body::from(body_str);
        ctx = ctx.resp(StatusCode::OK, HeaderMap::new(), resp_body);

        let (is_continue, mut ctx) = filter.resp_filter("", ctx, Some(&matched)).await.unwrap();
        assert!(is_continue);
        let resp_body = ctx.pop_resp_body().await.unwrap().unwrap();
        println!("===resp_body{:?}", resp_body);
        let mut decode = GzipDecoder::new(BufReader::new(&resp_body[..]));
        let mut encoder_body = vec![];
        let _ = decode.read_to_end(&mut encoder_body).await;
        println!("==={:?}", encoder_body);
    }
}
