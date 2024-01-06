use std::sync::Arc;

use tokio::sync::Mutex;
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
            current_service: inner,
            next_service: self.reloader.service.clone(),
        }
    }
}

#[derive(Debug, Clone)]
pub struct Reload<S>
where
    S: Send,
{
    current_service: S,
    next_service: Arc<Mutex<Option<S>>>,
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

impl<Request, S> Service<Request> for Reload<S>
where
    S: Service<Request> + Send,
{
    type Response = S::Response;

    type Error = S::Error;

    type Future = S::Future;

    fn poll_ready(&mut self, cx: &mut std::task::Context<'_>) -> std::task::Poll<Result<(), Self::Error>> {
        let Ok(mut next) = self.next_service.try_lock() else {
            return self.current_service.poll_ready(cx);
        };
        if let Some(new_service) = next.take() {
            self.current_service = new_service
        }
        drop(next);
        self.current_service.poll_ready(cx)
    }

    fn call(&mut self, req: Request) -> Self::Future {
        self.current_service.call(req)
    }
}
