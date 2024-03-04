use std::{cmp::Ordering, str::FromStr};

use hyper::header::HeaderValue;
use tower::BoxError;

use super::content_encoding::{ContentEncoding, ContentEncodingType};

#[derive(Debug, PartialEq, Eq)]
pub enum AcceptEncodingType {
    Gzip,
    Deflate,
    Br,
    Identity,
    Any,
}

#[derive(Debug)]
pub struct AcceptEncodingItem {
    r#type: AcceptEncodingType,
    q: f32,
}

#[derive(Debug, Default)]
pub struct AcceptEncoding {
    items: Vec<AcceptEncodingItem>,
}

impl AcceptEncodingType {
    pub const fn as_str(&self) -> &'static str {
        match self {
            AcceptEncodingType::Gzip => "gzip",
            AcceptEncodingType::Deflate => "deflate",
            AcceptEncodingType::Br => "br",
            AcceptEncodingType::Identity => "identity",
            AcceptEncodingType::Any => "*",
        }
    }
}

impl FromStr for AcceptEncodingType {
    type Err = BoxError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "gzip" => Ok(AcceptEncodingType::Gzip),
            "deflate" => Ok(AcceptEncodingType::Deflate),
            "br" => Ok(AcceptEncodingType::Br),
            "identity" => Ok(AcceptEncodingType::Identity),
            "*" => Ok(AcceptEncodingType::Any),
            _ => Err("Invalid compression type".into()),
        }
    }
}

impl From<&HeaderValue> for AcceptEncoding {
    fn from(header_value: &HeaderValue) -> Self {
        let s = header_value.to_str().unwrap_or_default();
        let v = s
            .split(',')
            .filter_map(|encode_item| {
                if let Some((encode, q_str)) = encode_item.split_once(';') {
                    let q = q_str.trim().strip_prefix("q=").and_then(|q| q.parse::<f32>().ok()).unwrap_or(1.0f32);
                    AcceptEncodingType::from_str(encode).ok().map(|encode| AcceptEncodingItem { r#type: encode, q })
                } else {
                    AcceptEncodingType::from_str(encode_item).ok().map(|encode| AcceptEncodingItem { r#type: encode, q: 1.0f32 })
                }
            })
            .collect::<Vec<_>>();
        AcceptEncoding { items: v }
    }
}

impl AcceptEncoding {
    pub fn is_compatible(&self, content_encoding: ContentEncodingType) -> bool {
        // if Content-Encoding are in Accept-Encoding, return true
        self.items.iter().any(|accept_encoding_item| accept_encoding_item.r#type == AcceptEncodingType::from(content_encoding))
    }

    pub fn accept_identity(&self) -> bool {
        !self.items.iter().any(|accept_encoding_item| accept_encoding_item.r#type == AcceptEncodingType::Identity && accept_encoding_item.q == 0.0f32)
    }

    pub fn get_preferred(&self) -> Option<AcceptEncodingType> {
        self.items
            .iter()
            .max_by_key(|item| if item.r#type == AcceptEncodingType::Any { Ordering::Less } else { Ordering::Greater })
            .map(|item| (item.q > 0.0).then_some(item.r#type))
            .flatten()
    }
}
