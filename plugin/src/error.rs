use std::{borrow::Cow, fmt::Display};

use hyper::{Response, StatusCode};
use spacegate_kernel::{SgBody, SgResponseExt};

use crate::Plugin;

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
        let message = format!("[Sg.Plugin.{p}] {e}", p = val.plugin_code, e = val.source);
        Response::with_code_message(val.status, message)
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
