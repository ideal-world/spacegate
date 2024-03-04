use std::str::FromStr;

use hyper::header::HeaderValue;
use tower::BoxError;

use super::accept_encoding::{AcceptEncoding, AcceptEncodingType};

pub struct ContentEncoding {
    // don't support multiple encoding
    pub r#type: ContentEncodingType,
}
#[derive(Debug, PartialEq, Eq, Clone, Copy)]
pub enum ContentEncodingType {
    Gzip,
    Deflate,
    Br,
}

impl ContentEncodingType {
    pub const fn as_str(&self) -> &'static str {
        match self {
            ContentEncodingType::Gzip => "gzip",
            ContentEncodingType::Deflate => "deflate",
            ContentEncodingType::Br => "br",
        }
    }
}

impl From<ContentEncodingType> for AcceptEncodingType {
    fn from(val: ContentEncodingType) -> Self {
        match val {
            ContentEncodingType::Gzip => AcceptEncodingType::Gzip,
            ContentEncodingType::Deflate => AcceptEncodingType::Deflate,
            ContentEncodingType::Br => AcceptEncodingType::Br,
        }
    }
}


impl TryFrom<&HeaderValue> for ContentEncodingType {
    type Error = Infallible;

    fn try_from(header_value: &HeaderValue) -> Result<Self, Self::Error> {
        match header_value.as_bytes() {
            b"gzip" => Ok(ContentEncodingType::Gzip),
            b"deflate" => Ok(ContentEncodingType::Deflate),
            b"br" => Ok(ContentEncodingType::Br),
            _ => Err("Invalid compression type".into()),
        }
    }
}
impl FromStr for ContentEncodingType {
    type Err = BoxError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "gzip" => Ok(ContentEncodingType::Gzip),
            "deflate" => Ok(ContentEncodingType::Deflate),
            "br" => Ok(ContentEncodingType::Br),
            _ => Err("Invalid compression type".into()),
        }
    }
}
