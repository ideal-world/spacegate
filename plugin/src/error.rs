use std::{borrow::Cow, fmt::Display};

use hyper::{Response, StatusCode};
use spacegate_kernel::{SgBody, SgResponseExt};

use crate::Plugin;

#[derive(Debug)]
pub struct PluginError<E> {
    plugin_code: Cow<'static, str>,
    source: E,
    status: StatusCode,
}

impl<E> From<PluginError<E>> for Response<SgBody>
where
    E: Display,
{
    fn from(val: PluginError<E>) -> Self {
        Response::with_code_message(val.status, val.to_string())
    }
}

impl<E> PluginError<E> {
    pub fn status<P: Plugin>(status: StatusCode) -> impl Fn(E) -> Self {
        move |e: E| Self {
            plugin_code: Cow::Borrowed(P::CODE),
            source: e,
            status,
        }
    }
    pub fn bad_gateway<P: Plugin>(e: E) -> Self {
        Self {
            plugin_code: Cow::Borrowed(P::CODE),
            source: e,
            status: StatusCode::BAD_GATEWAY,
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

impl<E> std::error::Error for PluginError<E> where E: std::error::Error + 'static {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        Some(&self.source)
    }
}
