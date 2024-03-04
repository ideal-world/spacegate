use crate::SgBody;
use hyper::{header::HeaderValue, Response};

pub fn with_length_or_chunked(resp: &mut Response<SgBody>) {
    if let Some(len) = resp.body().get_dumped().map(hyper::body::Bytes::len) {
        resp.headers_mut().insert(
            hyper::header::CONTENT_LENGTH,
            HeaderValue::from_str(len.to_string().as_str()).expect("digits should be valid header char"),
        );
    } else {
        // let is_chunked = resp.headers().get_all(hyper::header::TRANSFER_ENCODING).iter().any(|v| v.as_bytes() == b"chunked");
        // resp.headers_mut().append(hyper::header::TRANSFER_ENCODING, HeaderValue::from_static("chunked"));
    }
}
