use std::cmp::Ordering;

use async_compression::tokio::bufread::{BrotliDecoder, BrotliEncoder, DeflateDecoder, DeflateEncoder, GzipDecoder, GzipEncoder};
use async_trait::async_trait;
use http::{header, HeaderValue};
use hyper::Body;
use itertools::Itertools;
use serde::{Deserialize, Serialize};
use tardis::{
    basic::result::TardisResult,
    futures_util::{StreamExt, TryStreamExt},
    tokio::io::{AsyncReadExt, BufReader},
    TardisFuns,
};
use tokio_util::io::{ReaderStream, StreamReader};

use super::{BoxSgPluginFilter, SgPluginFilter, SgPluginFilterDef, SgPluginFilterInitDto, SgRoutePluginContext};

pub const CODE: &str = "compression";
pub struct SgFilterCompressionDef;

impl SgPluginFilterDef for SgFilterCompressionDef {
    fn inst(&self, spec: serde_json::Value) -> TardisResult<BoxSgPluginFilter> {
        let filter = TardisFuns::json.json_to_obj::<SgFilterCompression>(spec)?;
        Ok(filter.boxed())
    }
}

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

impl CompressionType {
    fn from_str(s: &str) -> Option<Self> {
        if s == CompressionType::Gzip {
            Some(CompressionType::Gzip)
        } else if s == CompressionType::Br {
            Some(CompressionType::Br)
        } else if s == CompressionType::Deflate {
            Some(CompressionType::Deflate)
        } else {
            None
        }
    }
}

#[async_trait]
impl SgPluginFilter for SgFilterCompression {
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

    async fn req_filter(&self, _: &str, ctx: SgRoutePluginContext) -> TardisResult<(bool, SgRoutePluginContext)> {
        Ok((true, ctx))
    }

    async fn resp_filter(&self, _: &str, mut ctx: SgRoutePluginContext) -> TardisResult<(bool, SgRoutePluginContext)> {
        // let resp_body = ctx.response.raw_body;
        let resp_encode_type = get_encode_type(ctx.response.get_headers_raw().get(header::CONTENT_ENCODING));
        let desired_response_encoding = get_encode_type(ctx.request.get_headers_raw().get(header::ACCEPT_ENCODING));

        fn convert_error(err: hyper::Error) -> std::io::Error {
            std::io::Error::new(std::io::ErrorKind::Other, err)
        }
        if let Some(mut body) = ctx.response.raw_body.take() {
            if desired_response_encoding != resp_encode_type {
                if let Some(resp_encode_type) = resp_encode_type {
                    let bytes_reader = StreamReader::new(body.map_err(convert_error));
                    body = match resp_encode_type {
                        CompressionType::Gzip => {
                            let decoded = GzipDecoder::new(bytes_reader);
                            let stream = ReaderStream::new(decoded);
                            Body::wrap_stream(stream)
                        }
                        CompressionType::Deflate => {
                            let decoded = DeflateDecoder::new(bytes_reader);
                            let stream = ReaderStream::new(decoded);
                            Body::wrap_stream(stream)
                        }
                        CompressionType::Br => {
                            let decoded = BrotliDecoder::new(bytes_reader);
                            let stream = ReaderStream::new(decoded);
                            Body::wrap_stream(stream)
                        }
                    };
                }
            }
            if let Some(desired_response_encoding) = desired_response_encoding {
                let bytes_reader = StreamReader::new(body.map_err(convert_error));
                body = match desired_response_encoding {
                    CompressionType::Gzip => {
                        ctx.response.set_header(header::CONTENT_ENCODING.as_str(), CompressionType::Gzip.into())?;
                        Body::wrap_stream(ReaderStream::new(GzipEncoder::new(bytes_reader)))
                    }
                    CompressionType::Deflate => {
                        ctx.response.set_header(header::CONTENT_ENCODING.as_str(), CompressionType::Deflate.into())?;
                        Body::wrap_stream(ReaderStream::new(DeflateEncoder::new(bytes_reader)))
                    }
                    CompressionType::Br => {
                        ctx.response.set_header(header::CONTENT_ENCODING.as_str(), CompressionType::Br.into())?;
                        Body::wrap_stream(ReaderStream::new(BrotliEncoder::new(bytes_reader)))
                    }
                }
            }
            ctx.response.raw_body.replace(body);
        }
        Ok((true, ctx))
    }
}

fn get_encode_type(header_value: Option<&HeaderValue>) -> Option<CompressionType> {
    if let Some(header_value) = header_value {
        header_value.to_str().map_or_else(
            |_| None,
            |v_str| {
                // support ;q=
                if v_str.contains(";q=") {
                    let highest_q = v_str
                        .split(',')
                        .map(|s| s.trim())
                        .map(|s| {
                            if let Some((comp_type, q)) = s.split_once(";q=") {
                                (q.parse::<f32>().unwrap_or(1f32), CompressionType::from_str(comp_type))
                            } else {
                                (1f32, CompressionType::from_str(s))
                            }
                        })
                        .max_by(|(q1, t1), (q2, t2)| {
                            if t1.is_none() && t2.is_none() {
                                Ordering::Equal
                            } else if t1.is_none() && t2.is_some() {
                                Ordering::Less
                            } else if t1.is_some() && t2.is_none() {
                                Ordering::Greater
                            } else {
                                q1.total_cmp(q2)
                            }
                        });
                    if let Some(first) = highest_q {
                        first.1.clone()
                    } else {
                        None
                    }
                } else if !v_str.is_empty() {
                    let mut result = None;
                    for s in v_str.split(',').map(|s| s.trim()) {
                        result = CompressionType::from_str(s);
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
#[allow(clippy::unwrap_used)]
mod tests {

    use super::*;
    use async_compression::tokio::bufread::GzipDecoder;
    use http::{HeaderMap, Method, StatusCode, Uri, Version};
    use hyper::Body;
    use tardis::tokio::{self};

    #[test]
    fn test_get_encode_type() {
        assert_eq!(get_encode_type(None), None);
        assert_eq!(get_encode_type(Some(&HeaderValue::from_static("identity"))), None);
        assert_eq!(get_encode_type(Some(&HeaderValue::from_static("*"))), None);
        assert_eq!(get_encode_type(Some(&HeaderValue::from_static("gzip, deflate, br"))), Some(CompressionType::Gzip));
        assert_eq!(
            get_encode_type(Some(&HeaderValue::from_static("br;q=0.2, gzip;q=0.8, *;q=0.1"))),
            Some(CompressionType::Gzip)
        );
    }

    #[tokio::test]
    async fn test_gzip() {
        let filter = SgFilterCompression {};

        let mut header = HeaderMap::new();
        header.insert(header::ACCEPT_ENCODING, CompressionType::Gzip.into());
        let ctx = SgRoutePluginContext::new_http(
            Method::POST,
            Uri::from_static("http://sg.idealworld.group/"),
            Version::HTTP_11,
            header,
            Body::empty(),
            "127.0.0.1:8080".parse().unwrap(),
            "".to_string(),
            None,
        );

        let (is_continue, mut ctx) = filter.req_filter("", ctx).await.unwrap();
        assert!(is_continue);

        let body_str = "test 1 测试 1 ";
        let resp_body = Body::from(body_str);
        ctx = ctx.resp(StatusCode::OK, HeaderMap::new(), resp_body);

        let (is_continue, mut ctx) = filter.resp_filter("", ctx).await.unwrap();
        assert!(is_continue);
        let resp_body = hyper::body::to_bytes(ctx.response.raw_body.unwrap()).await.unwrap();
        dbg!(&resp_body);
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
        let ctx = SgRoutePluginContext::new_http(
            Method::POST,
            Uri::from_static("http://sg.idealworld.group/"),
            Version::HTTP_11,
            header,
            Body::empty(),
            "127.0.0.1:8080".parse().unwrap(),
            "".to_string(),
            None,
        );

        let (is_continue, mut ctx) = filter.req_filter("", ctx).await.unwrap();
        assert!(is_continue);

        let body_str = "test 1 测试 1 ";
        let mut dncoder = DeflateEncoder::new(BufReader::new(body_str.as_bytes()));
        let mut dncoded_body = vec![];
        let _ = dncoder.read_to_end(&mut dncoded_body).await;
        let resp_body = Body::from(dncoded_body);
        let mut mock_resp_header = HeaderMap::new();
        mock_resp_header.insert(header::CONTENT_ENCODING, CompressionType::Deflate.into());
        ctx = ctx.resp(StatusCode::OK, mock_resp_header, resp_body);

        let (is_continue, mut ctx) = filter.resp_filter("", ctx).await.unwrap();
        assert!(is_continue);

        let resp_body = hyper::body::to_bytes(ctx.response.raw_body.unwrap()).await.unwrap();
        let mut decode = GzipDecoder::new(BufReader::new(&*resp_body));
        let mut decoded_body = vec![];
        let _ = decode.read_to_end(&mut decoded_body).await;
        dbg!(&decoded_body);
        unsafe {
            let body = String::from_utf8_unchecked(decoded_body);
            assert_eq!(&body, body_str);
        }
    }
}
