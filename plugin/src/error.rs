use std::{borrow::Cow, fmt::Display, num::NonZeroU16};

use hyper::{header::HeaderValue, Response, StatusCode};
use spacegate_kernel::{SgBody, SgResponseExt};

use crate::Plugin;

#[derive(Debug)]
pub struct PluginError<E> {
    plugin_code: &'static str,
    source: E,
    status: StatusCode,
}

const PLUGIN_ERROR_HEADER: &str = "X-Plugin-Error";


impl<E> From<PluginError<E>> for Response<SgBody>
where
    E: Display,
{
    fn from(val: PluginError<E>) -> Self {
        let mut resp = Response::with_code_message(val.status, val.to_string());
        resp.headers_mut().insert(PLUGIN_ERROR_HEADER, HeaderValue::from_static(val.plugin_code));
        resp
    }
}

impl<E> PluginError<E> {
    pub fn status<P: Plugin, const S: u16>(error: E) -> Self {
        move |e: E| Self {
            plugin_code: P::CODE,
            source: e,
            status: StatusCode::from_u16(S),
        }
    }
    pub fn internal_error<P: Plugin>(e: E) -> Self {
        Self {
            plugin_code: P::CODE,
            source: e,
            status: StatusCode::INTERNAL_SERVER_ERROR,
        }
    }
}

impl<E> std::fmt::Display for PluginError<E>
where
    E: std::fmt::Display,
{
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "[Sg.Plugin.{p}] {e}", p = self.plugin_code, e = self.source)
    }
}

impl<E> std::error::Error for PluginError<E>
where
    E: std::error::Error + 'static,
{
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        Some(&self.source)
    }
}
