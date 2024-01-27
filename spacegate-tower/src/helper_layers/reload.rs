use std::sync::Arc;

use futures_util::future::BoxFuture;
use tokio::sync::{Mutex, RwLock};
use tower_layer::Layer;

use tower::Service;

#[derive(Default, Debug, Clone)]
pub struct ReloadLayer<S> {
    reloader: Reloader<S>,
}

impl<S> Layer<S> for ReloadLayer<S>
where
    S: Send,
{
    type Service = Reload<S>;

    fn layer(&self, inner: S) -> Self::Service {
        Reload {
            service: Arc::new(RwLock::new(inner)),
        }
    }
}

#[derive(Debug, Clone)]
pub struct Reload<S>
where
    S: Send,
{
    service: Arc<RwLock<S>>,
}

#[derive(Debug)]
pub struct Reloader<S> {
    pub service: Arc<Mutex<Option<S>>>,
}

impl<S> Default for Reloader<S> {
    fn default() -> Self {
        Self { service: Arc::default() }
    }
}

impl<S> Clone for Reloader<S> {
    fn clone(&self) -> Self {
        Self { service: self.service.clone() }
    }
}

impl<S> Reloader<S> {
    pub async fn reload(&self, service: S) -> Option<S> {
        let mut wg = self.service.lock().await;
        wg.replace(service)
    }
    pub fn into_layer(self) -> ReloadLayer<S> {
        ReloadLayer { reloader: self }
    }
}

impl<Request, S> hyper::service::Service<Request> for Reload<S>
where
    Request: Send + Sync + 'static,
    S: hyper::service::Service<Request> + Send + Sync + 'static,
    <S as hyper::service::Service<Request>>::Future: std::marker::Send,
{
    type Response = S::Response;

    type Error = S::Error;

    type Future = BoxFuture<'static, Result<S::Response, S::Error>>;

    fn call(&self, req: Request) -> Self::Future {
        let service = self.service.clone();
        Box::pin(async move {
            let rg = service.read_owned().await;
            rg.call(req).await
        })
    }
}
