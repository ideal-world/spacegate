use std::{borrow::Cow, collections::HashMap};

use hyper::header::HeaderValue;
use spacegate_plugin::{BoxError, Inner, Plugin, PluginDefinitionObject, SgRequest, SgResponse};
pub struct SayHelloPlugin;

impl Plugin for SayHelloPlugin {
    const CODE: &'static str = "sayhello";

    async fn call(&self, mut req: SgRequest, inner: Inner) -> Result<SgResponse, BoxError> {
        req.headers_mut().insert("hello", HeaderValue::from_static("world"));
        Ok(inner.call(req).await)
    }

    fn create(_plugin_config: spacegate_plugin::PluginConfig) -> Result<Self, BoxError> {
        Ok(Self)
    }
}

#[no_mangle]
pub extern "Rust" fn register_fn_list() -> &'static [(&'static str, fn() -> spacegate_plugin::PluginDefinitionObject)] {
    &[("sayhello", PluginDefinitionObject::from_trait::<SayHelloPlugin>)]
}
