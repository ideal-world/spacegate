use crate::{BoxError, BoxResult};

use hyper::http::HeaderValue;

use super::Authorization;

#[derive(Debug)]
pub struct Bearer {
    pub token: String,
}

impl Bearer {
    pub fn new(token: impl Into<String>) -> Self {
        Self { token: token.into() }
    }
    ///
    /// # Errors
    ///
    /// If the token is not a valid header value.
    pub fn to_header(&self) -> BoxResult<hyper::http::HeaderValue> {
        Ok(hyper::http::HeaderValue::from_str(&format!("Bearer {}", self.token))?)
    }
}

impl From<Bearer> for Authorization<Bearer> {
    fn from(val: Bearer) -> Self {
        Authorization(val)
    }
}

impl From<Authorization<Bearer>> for Bearer {
    fn from(val: Authorization<Self>) -> Self {
        val.0
    }
}

impl TryFrom<HeaderValue> for Bearer {
    type Error = BoxError;
    fn try_from(header: HeaderValue) -> Result<Self, Self::Error> {
        if let Ok(header) = header.to_str() {
            if let Some(token) = header.strip_prefix("Bearer ") {
                Ok(Bearer::new(token))
            } else {
                Err("auth header value is not a bearer auth value".into())
            }
        } else {
            Err("auth header value is not a valid string".into())
        }
    }
}

impl TryInto<HeaderValue> for &Bearer {
    type Error = BoxError;
    fn try_into(self) -> Result<HeaderValue, Self::Error> {
        self.to_header()
    }
}

impl std::fmt::Display for Bearer {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Bearer {}", self.token)
    }
}
