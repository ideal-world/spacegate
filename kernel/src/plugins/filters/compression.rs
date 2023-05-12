use async_compression::tokio::bufread::{BrotliDecoder, BrotliEncoder, DeflateDecoder, DeflateEncoder, GzipDecoder, GzipEncoder};
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
impl PartialEq<CompressionType> for &str {
    fn eq(&self, other: &CompressionType) -> bool {
        let other_str: &str = other.clone().into();
        self.to_lowercase() == *other_str
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
        if let Some(mut resp_body) = resp_body {
            let resp_encode_type = get_encode_type(ctx.get_resp_headers_raw().get(header::CONTENT_ENCODING));
            let desired_response_encoding = get_encode_type(ctx.get_req_headers_raw().get(header::ACCEPT_ENCODING));
            if desired_response_encoding == resp_encode_type {
                ctx.set_resp_body(resp_body)?;
                return Ok((true, ctx));
            } else {
                let mut decoded_body = vec![];
                if let Some(resp_encode_type) = resp_encode_type {
                    match resp_encode_type {
                        CompressionType::Gzip => {
                            let mut decoded = GzipDecoder::new(BufReader::new(&resp_body[..]));
                            let _ = decoded.read_to_end(&mut decoded_body).await;
                        }
                        CompressionType::Deflate => {
                            let mut decoded = DeflateDecoder::new(BufReader::new(&resp_body[..]));
                            let _ = decoded.read_to_end(&mut decoded_body).await;
                        }
                        CompressionType::Br => {
                            let mut decoded = BrotliDecoder::new(BufReader::new(&resp_body[..]));
                            let _ = decoded.read_to_end(&mut decoded_body).await;
                        }
                    }
                    resp_body = decoded_body;
                }
            }
            if let Some(desired_response_encoding) = desired_response_encoding {
                let mut encoded_body = vec![];
                match desired_response_encoding {
                    CompressionType::Gzip => {
                        ctx.set_resp_header(header::CONTENT_ENCODING.as_str(), CompressionType::Gzip.into())?;
                        let mut encoded = GzipEncoder::new(BufReader::new(&resp_body[..]));
                        let _ = encoded.read_to_end(&mut encoded_body).await;
                    }
                    CompressionType::Deflate => {
                        ctx.set_resp_header(header::CONTENT_ENCODING.as_str(), CompressionType::Deflate.into())?;
                        let mut encoded = DeflateEncoder::new(BufReader::new(&resp_body[..]));
                        let _ = encoded.read_to_end(&mut encoded_body).await;
                    }
                    CompressionType::Br => {
                        ctx.set_resp_header(header::CONTENT_ENCODING.as_str(), CompressionType::Br.into())?;
                        let mut encoded = BrotliEncoder::new(BufReader::new(&resp_body[..]));
                        let _ = encoded.read_to_end(&mut encoded_body).await;
                    }
                }
                ctx.set_resp_body(encoded_body)?;
                return Ok((true, ctx));
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
                let split: Vec<&str> = v_str.split(',').collect();
                if v_str.contains(";q=") {
                    let result = None;
                    for s in split {
                        let split: Vec<&str> = v_str.split(";q=").collect();
                        if split.len() == 2 {
                            //TODO support ;q=
                        }
                    }
                    result
                } else if !split.is_empty() {
                    let mut result = None;
                    for s in split {
                        result = if s == CompressionType::Gzip {
                            Some(CompressionType::Gzip)
                        } else if s == CompressionType::Br {
                            Some(CompressionType::Br)
                        } else if s == CompressionType::Deflate {
                            Some(CompressionType::Deflate)
                        } else {
                            None
                        };
                        if result.is_some() {
                            break;
                        }
                    }
                    result
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
        header.insert(header::ACCEPT_ENCODING, CompressionType::Gzip.into());
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
        let mut decode = GzipDecoder::new(BufReader::new(&resp_body[..]));
        let mut encoder_body = vec![];
        let _ = decode.read_to_end(&mut encoder_body).await;
        unsafe {
            let body = String::from_utf8_unchecked(encoder_body);
            assert_eq!(&body, body_str);
        }
    }

    #[tokio::test]
    async fn test_convert_compression() {
        //gzip -> deflate
        let filter = SgFilterCompression {};

        let mut header = HeaderMap::new();
        header.insert(header::ACCEPT_ENCODING, CompressionType::Gzip.into());
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
        let mut dncoder = DeflateEncoder::new(BufReader::new(body_str.as_bytes()));
        let mut dncoded_body = vec![];
        let _ = dncoder.read_to_end(&mut dncoded_body).await;
        let resp_body = Body::from(dncoded_body);
        let mut mock_resp_header = HeaderMap::new();
        mock_resp_header.insert(header::CONTENT_ENCODING, CompressionType::Deflate.into());
        ctx = ctx.resp(StatusCode::OK, mock_resp_header, resp_body);

        let (is_continue, mut ctx) = filter.resp_filter("", ctx, Some(&matched)).await.unwrap();
        assert!(is_continue);

        let resp_body = ctx.pop_resp_body().await.unwrap().unwrap();
        let mut decode = GzipDecoder::new(BufReader::new(&resp_body[..]));
        let mut decoded_body = vec![];
        let _ = decode.read_to_end(&mut decoded_body).await;
        unsafe {
            let body = String::from_utf8_unchecked(decoded_body);
            assert_eq!(&body, body_str);
        }
    }
}
