use hyper::header::HeaderValue;
use spacegate_plugin::{dynamic_lib, BoxError, Inner, Plugin, SgRequest, SgResponse};
pub struct SayHelloPlugin;

impl Plugin for SayHelloPlugin {
    const CODE: &'static str = "sayhello";

    async fn call(&self, req: SgRequest, inner: Inner) -> Result<SgResponse, BoxError> {
        let mut resp = inner.call(req).await;
        resp.headers_mut().insert("hello", HeaderValue::from_static("world"));
        Ok(resp)
    }

    fn create(_plugin_config: spacegate_plugin::PluginConfig) -> Result<Self, BoxError> {
        Ok(Self)
    }
}

dynamic_lib! {
    SayHelloPlugin
}
