use crate::SgBody;
use hyper::{header::HeaderValue, Response};

/// Set the content length header or transfer encoding to chunked.
pub fn with_length_or_chunked(resp: &mut Response<SgBody>) {
    const CHUNKED: &str = "chunked";
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
