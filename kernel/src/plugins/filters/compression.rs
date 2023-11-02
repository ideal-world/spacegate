use std::{cmp::Ordering, pin::Pin};

use crate::def_filter;
use async_compression::tokio::bufread::{BrotliDecoder, BrotliEncoder, DeflateDecoder, DeflateEncoder, GzipDecoder, GzipEncoder};
use async_trait::async_trait;
use http::{header, HeaderValue};
use hyper::Body;
use serde::{Deserialize, Serialize};
use tardis::{basic::result::TardisResult, futures_util::TryStreamExt, tokio::io::BufReader};
use tokio_util::io::{ReaderStream, StreamReader};

use super::{SgPluginFilter, SgPluginFilterInitDto, SgRoutePluginContext};

def_filter!("compression", SgFilterCompressionDef, SgFilterCompression);

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
        self.eq_ignore_ascii_case(other_str)
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

    async fn req_filter(&self, _: &str, mut ctx: SgRoutePluginContext) -> TardisResult<(bool, SgRoutePluginContext)> {
        let desired_response_encoding = get_encode_type(ctx.request.get_headers_raw().get(header::ACCEPT_ENCODING));
        if let Some(encode) = desired_response_encoding {
            ctx.request.set_header(header::ACCEPT_ENCODING, encode.into())?;
        }
        Ok((true, ctx))
    }

    async fn resp_filter(&self, _: &str, mut ctx: SgRoutePluginContext) -> TardisResult<(bool, SgRoutePluginContext)> {
        let resp_encode_type = get_encode_type(ctx.response.get_headers_raw().get(header::CONTENT_ENCODING));
        let desired_response_encoding = get_encode_type(ctx.request.get_headers_raw().get(header::ACCEPT_ENCODING));
        fn convert_error(err: hyper::Error) -> std::io::Error {
            std::io::Error::new(std::io::ErrorKind::Other, err)
        }
        if desired_response_encoding == resp_encode_type {
            return Ok((true, ctx));
        }
        if let Some(desired_response_encoding) = &desired_response_encoding {
            match desired_response_encoding {
                CompressionType::Gzip => ctx.response.set_header(header::CONTENT_ENCODING, CompressionType::Gzip.into())?,
                CompressionType::Deflate => ctx.response.set_header(header::CONTENT_ENCODING, CompressionType::Deflate.into())?,
                CompressionType::Br => ctx.response.set_header(header::CONTENT_ENCODING, CompressionType::Br.into())?,
            }
        }
        let mut body = ctx.response.take_body();
        body = if let Some(resp_encode_type) = resp_encode_type {
            ctx.response.remove_header(header::CONTENT_LENGTH)?;
            let bytes_reader = StreamReader::new(body.map_err(convert_error));
            let mut read_stream: Pin<Box<dyn tardis::tokio::io::AsyncRead + Send>> = match resp_encode_type {
                CompressionType::Gzip => Box::pin(GzipDecoder::new(bytes_reader)),
                CompressionType::Deflate => Box::pin(DeflateDecoder::new(bytes_reader)),
                CompressionType::Br => Box::pin(BrotliDecoder::new(bytes_reader)),
            };
            if let Some(desired_response_encoding) = desired_response_encoding {
                read_stream = match desired_response_encoding {
                    CompressionType::Gzip => Box::pin(GzipEncoder::new(BufReader::new(read_stream))),
                    CompressionType::Deflate => Box::pin(DeflateEncoder::new(BufReader::new(read_stream))),
                    CompressionType::Br => Box::pin(BrotliEncoder::new(BufReader::new(read_stream))),
                };
            }
            Body::wrap_stream(ReaderStream::new(read_stream))
        } else if let Some(desired_response_encoding) = desired_response_encoding {
            ctx.response.remove_header(header::CONTENT_LENGTH)?;
            let bytes_reader = StreamReader::new(body.map_err(convert_error));
            match desired_response_encoding {
                CompressionType::Gzip => Body::wrap_stream(ReaderStream::new(GzipEncoder::new(bytes_reader))),
                CompressionType::Deflate => Body::wrap_stream(ReaderStream::new(DeflateEncoder::new(bytes_reader))),
                CompressionType::Br => Body::wrap_stream(ReaderStream::new(BrotliEncoder::new(bytes_reader))),
            }
        } else {
            body
        };

        ctx.response.set_body(body);
        // let body = ctx.response.dump_body().await?;
        // dbg!(body);
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

mod tests {

    use super::*;
    use async_compression::tokio::bufread::GzipDecoder;
    use http::{HeaderMap, Method, StatusCode, Uri, Version};
    use hyper::Body;
    use tardis::tokio::{self, io::AsyncReadExt};
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
            None,
        );

        let (is_continue, mut ctx) = filter.req_filter("", ctx).await.unwrap();
        assert!(is_continue);

        let body_str = "test 1 测试 1 ";
        let resp_body = Body::from(body_str);
        ctx = ctx.resp(StatusCode::OK, HeaderMap::new(), resp_body);

        let (is_continue, mut ctx) = filter.resp_filter("", ctx).await.unwrap();
        assert!(is_continue);
        let resp_body = ctx.response.dump_body().await.unwrap();
        let mut decode = GzipDecoder::new(BufReader::new(&*resp_body));
        let mut encoder_body = vec![];
        let _ = decode.read_to_end(&mut encoder_body).await;
        // unsafe in test would be ok
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
            None,
        );

        let (is_continue, mut ctx) = filter.req_filter("", ctx).await.unwrap();
        assert!(is_continue);

        let body_str = "test 1 测试 1 ";
        let mut deflate_encoder = DeflateEncoder::new(BufReader::new(body_str.as_bytes()));
        let mut deflate_encoded_body = vec![];
        let _ = deflate_encoder.read_to_end(&mut deflate_encoded_body).await;
        let resp_body = Body::from(deflate_encoded_body);
        let mut mock_resp_header = HeaderMap::new();
        mock_resp_header.insert(header::CONTENT_ENCODING, CompressionType::Deflate.into());
        ctx = ctx.resp(StatusCode::OK, mock_resp_header, resp_body);

        let (is_continue, mut ctx) = filter.resp_filter("", ctx).await.unwrap();
        assert!(is_continue);

        let resp_body = ctx.response.dump_body().await.unwrap();
        let mut decode = GzipDecoder::new(BufReader::new(&*resp_body));
        let mut decoded_body = vec![];
        let _ = decode.read_to_end(&mut decoded_body).await;
        let decoded_body = String::from_utf8(decoded_body).unwrap();
        assert_eq!(&decoded_body, body_str);
    }
}
