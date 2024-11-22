use std::{convert::Infallible, str::FromStr};

use base64::Engine;
use hyper::http::HeaderValue;

use crate::{BoxError, BoxResult};

use super::Authorization;

#[derive(Debug, Clone)]
pub struct Basic {
    pub username: String,
    pub password: Option<String>,
}

impl From<Authorization<Basic>> for Basic {
    fn from(val: Authorization<Self>) -> Self {
        val.0
    }
}

impl From<Basic> for Authorization<Basic> {
    fn from(val: Basic) -> Self {
        Authorization(val)
    }
}

impl TryInto<HeaderValue> for &Basic {
    type Error = BoxError;
    fn try_into(self) -> Result<HeaderValue, Self::Error> {
        self.to_header()
    }
}

impl TryFrom<HeaderValue> for Basic {
    type Error = BoxError;
    fn try_from(header: HeaderValue) -> Result<Self, Self::Error> {
        if let Ok(header) = header.to_str() {
            if let Some(base64_auth) = header.strip_prefix("Basic ") {
                let auth = base64::engine::general_purpose::STANDARD.decode(base64_auth)?;
                let auth = String::from_utf8(auth)?;
                return Ok(Basic::infallible_parse(auth.as_str()));
            } else {
                Err("auth header value is not a basic auth value".into())
            }
        } else {
            Err("auth header value is not a valid string".into())
        }
    }
}

impl std::fmt::Display for Basic {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if let Some(password) = &self.password {
            write!(f, "{}:{}", self.username, password)
        } else {
            write!(f, "{}", self.username)
        }
    }
}

impl Basic {
    pub fn new(username: String, password: Option<String>) -> Self {
        Self { username, password }
    }
    pub fn to_auth_string(&self) -> String {
        match &self.password {
            Some(password) => format!("{}:{}", self.username, password),
            None => self.username.clone(),
        }
    }
    /// # Errors
    /// If the token is not a valid header value.
    pub fn to_header(&self) -> BoxResult<HeaderValue> {
        let auth_string = self.to_auth_string();
        let mut header_str = String::with_capacity(auth_string.len() + 10);
        header_str.push_str("Basic ");
        base64::engine::general_purpose::STANDARD.encode_string(auth_string, &mut header_str);
        let header = HeaderValue::from_maybe_shared(header_str)?;
        Ok(header)
    }
    pub fn infallible_parse(auth: &str) -> Self {
        if let Some((username, password)) = auth.split_once(':') {
            Self::new(username.to_string(), Some(password.to_string()))
        } else {
            Self::new(auth.to_string(), None)
        }
    }
}

impl FromStr for Basic {
    type Err = Infallible;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(Basic::infallible_parse(s))
    }
}
