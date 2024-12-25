use crate::SgBody;
use hyper::{body::Body, header::HeaderValue, Request, Response};

const CHUNKED: &str = "chunked";
#[allow(dead_code)]
/// Set the content length header or transfer encoding to chunked for Request.
pub fn req_length_or_chunked(req: &mut Request<SgBody>) {
    if let Some(len) = req.body().size_hint().exact() {
        req.headers_mut().remove(hyper::header::TRANSFER_ENCODING);
        req.headers_mut().insert(
            hyper::header::CONTENT_LENGTH,
            HeaderValue::from_str(len.to_string().as_str()).expect("digits should be valid header char"),
        );
    } else {
        req.headers_mut().remove(hyper::header::CONTENT_LENGTH);
        if let Some(len) = req.body().get_dumped().map(hyper::body::Bytes::len) {
            req.headers_mut().remove(hyper::header::TRANSFER_ENCODING);
            req.headers_mut().insert(
                hyper::header::CONTENT_LENGTH,
                HeaderValue::from_str(len.to_string().as_str()).expect("digits should be valid header char"),
            );
        } else if !req.headers().get_all(hyper::header::TRANSFER_ENCODING).iter().any(|v| v.as_bytes() == CHUNKED.as_bytes()) {
            req.headers_mut().append(hyper::header::TRANSFER_ENCODING, HeaderValue::from_static(CHUNKED));
        }
    }
}

/// Set the content length header or transfer encoding to chunked for Response.
pub fn with_length_or_chunked(resp: &mut Response<SgBody>) {
    if let Some(len) = resp.body().size_hint().exact() {
        resp.headers_mut().remove(hyper::header::TRANSFER_ENCODING);
        resp.headers_mut().insert(
            hyper::header::CONTENT_LENGTH,
            HeaderValue::from_str(len.to_string().as_str()).expect("digits should be valid header char"),
        );
    } else {
        resp.headers_mut().remove(hyper::header::CONTENT_LENGTH);
        if let Some(len) = resp.body().get_dumped().map(hyper::body::Bytes::len) {
            resp.headers_mut().remove(hyper::header::TRANSFER_ENCODING);
            resp.headers_mut().insert(
                hyper::header::CONTENT_LENGTH,
                HeaderValue::from_str(len.to_string().as_str()).expect("digits should be valid header char"),
            );
        } else if !resp.headers().get_all(hyper::header::TRANSFER_ENCODING).iter().any(|v| v.as_bytes() == CHUNKED.as_bytes()) {
            resp.headers_mut().append(hyper::header::TRANSFER_ENCODING, HeaderValue::from_static(CHUNKED));
        }
    }
}
