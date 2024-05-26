use std::sync::Arc;

use arc_swap::ArcSwap;
use futures_util::future::BoxFuture;
use hyper::{Request, Response};
use spacegate_kernel::{
    helper_layers::function::{FnLayerMethod, Inner},
    SgBody,
};

#[derive(Clone)]
pub struct PluginFunction {
    f: Arc<ArcSwap<InnerBoxPf>>,
}

pub(crate) type InnerBoxPf = Box<dyn Fn(Request<SgBody>, Inner) -> BoxFuture<'static, Response<SgBody>> + Send + Sync + 'static>;
impl PluginFunction {
    pub fn new(f: InnerBoxPf) -> Self {
        Self {
            f: Arc::new(ArcSwap::from_pointee(f)),
        }
    }
}

impl PluginFunction {
    pub fn swap(&self, f: Box<dyn Fn(Request<SgBody>, Inner) -> BoxFuture<'static, Response<SgBody>> + Send + Sync + 'static>) {
        self.f.store(f.into());
    }
}

impl FnLayerMethod for PluginFunction {
    async fn call(&self, req: Request<SgBody>, inner: Inner) -> Response<SgBody> {
        let f = self.f.load();
        f(req, inner).await
    }
}
