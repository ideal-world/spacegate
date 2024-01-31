use std::str::FromStr;

use hyper::header::HeaderValue;
use serde::{Deserialize, Serialize};
use tower::BoxError;

#[derive(Default, Debug, Serialize, Deserialize, Clone, PartialEq)]
pub enum CompressionType {
    #[default]
    Gzip,
    Deflate,
    Br,
}

impl CompressionType {
    pub const fn as_bytes(&self) -> &'static [u8] {
        match self {
            CompressionType::Gzip => b"gzip",
            CompressionType::Deflate => b"deflate",
            CompressionType::Br => b"br",
        }
    }

    pub const fn as_str(&self) -> &'static str {
        match self {
            CompressionType::Gzip => "gzip",
            CompressionType::Deflate => "deflate",
            CompressionType::Br => "br",
        }
    }
}

impl From<CompressionType> for HeaderValue {
    #[inline]
    fn from(algo: CompressionType) -> Self {
        HeaderValue::from_static(algo.as_str())
    }
}

impl From<CompressionType> for &str {
    #[inline]
    fn from(algo: CompressionType) -> Self {
        algo.as_str()
    }
}

impl PartialEq<CompressionType> for &str {
    fn eq(&self, other: &CompressionType) -> bool {
        let other_str: &str = other.clone().into();
        self.eq_ignore_ascii_case(other_str)
    }
}

impl FromStr for CompressionType {
    type Err = ();

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        if s.eq_ignore_ascii_case(CompressionType::Gzip.as_str()) {
            Ok(CompressionType::Gzip)
        } else if s.eq_ignore_ascii_case(CompressionType::Br.as_str()) {
            Ok(CompressionType::Br)
        } else if s.eq_ignore_ascii_case(CompressionType::Deflate.as_str()) {
            Ok(CompressionType::Deflate)
        } else {
            Err(())
        }
    }
}

impl TryFrom<&[u8]> for CompressionType {
    type Error = ();

    fn try_from(bytes: &[u8]) -> Result<Self, Self::Error> {
        if bytes.eq_ignore_ascii_case(CompressionType::Gzip.as_bytes()) {
            Ok(CompressionType::Gzip)
        } else if bytes.eq_ignore_ascii_case(CompressionType::Br.as_bytes()) {
            Ok(CompressionType::Br)
        } else if bytes.eq_ignore_ascii_case(CompressionType::Deflate.as_bytes()) {
            Ok(CompressionType::Deflate)
        } else {
            Err(())
        }
    }
}

impl TryFrom<&HeaderValue> for CompressionType {
    type Error = Infallible;

    fn try_from(header_value: &HeaderValue) -> Result<Self, Self::Error> {
        let s = header_value.to_str()?;
        s.split(',')
            .filter_map(|encode_item| {
                if let Some((encode, q_str)) = encode_item.split_once(';') {
                    let q = q_str.trim().strip_prefix("q=").and_then(|q| q.parse::<f32>().ok()).unwrap_or(1.0f32);
                    CompressionType::from_str(encode).ok().map(|encode| (encode, q))
                } else {
                    CompressionType::from_str(encode_item).ok().map(|encode| (encode, 1.0f32))
                }
            })
            .fold(None, |acc, (encode, q)| {
                if let Some((_, acc_q)) = acc {
                    if q > acc_q {
                        Some((encode, q))
                    } else {
                        acc
                    }
                } else {
                    Some((encode, q))
                }
            })
            .map(|(encode, _)| encode)
            .ok_or_else(|| "Invalid compression type".into())
    }
}
