use std::convert::Infallible;
use std::sync::Arc;

use futures_util::future::BoxFuture;
use futures_util::Future;
use hyper::{header::UPGRADE, Request, Response, StatusCode};
use tracing::instrument;

use crate::backend_service::http_client_service::get_client;
use crate::helper_layers::map_future::MapFuture;
use crate::utils::x_forwarded_for;
use crate::BoxError;
use crate::SgBody;
use crate::SgRequest;
use crate::SgResponse;
use crate::SgResponseExt;

pub mod echo;
pub mod http_client_service;
pub mod static_file_service;
pub mod ws_client_service;
pub trait SharedHyperService:
    hyper::service::Service<SgRequest, Response = SgResponse, Error = Infallible, Future = BoxFuture<'static, Result<SgResponse, Infallible>>> + Send + Sync + 'static
{
}

impl<T> SharedHyperService for T where
    T: hyper::service::Service<SgRequest, Response = SgResponse, Error = Infallible, Future = BoxFuture<'static, Result<SgResponse, Infallible>>> + Send + Sync + 'static
{
}
/// a service that can be shared between threads
pub struct ArcHyperService {
    pub shared: Arc<dyn SharedHyperService>,
}

impl std::fmt::Debug for ArcHyperService {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ArcHyperService").finish()
    }
}

impl Clone for ArcHyperService {
    fn clone(&self) -> Self {
        Self { shared: self.shared.clone() }
    }
}

impl ArcHyperService {
    pub fn new<T>(service: T) -> Self
    where
        T: hyper::service::Service<SgRequest, Response = SgResponse, Error = Infallible> + Send + Sync + 'static,
        T::Future: Future<Output = Result<Response<SgBody>, Infallible>> + 'static + Send,
    {
        let map_fut = MapFuture::new(service, |fut| Box::pin(fut) as _);
        Self { shared: Arc::new(map_fut) }
    }
    pub fn from_shared(shared: impl Into<Arc<dyn SharedHyperService>>) -> Self {
        Self { shared: shared.into() }
    }
}

impl hyper::service::Service<Request<SgBody>> for ArcHyperService {
    type Response = Response<SgBody>;
    type Error = Infallible;
    type Future = BoxFuture<'static, Result<Self::Response, Self::Error>>;

    #[inline]
    fn call(&self, req: Request<SgBody>) -> Self::Future {
        Box::pin(self.shared.call(req))
    }
}

/// Http backend service
///
/// This function could be a bottom layer of a http router, it will handle http and websocket request.
///
/// This can handle both websocket connection and http request.
///
/// # Errors
/// 1. Fail to collect body chunks
/// 2. Fail to upgrade
pub async fn http_backend_service_inner(mut req: Request<SgBody>) -> Result<SgResponse, BoxError> {
    tracing::trace!(elapsed = ?req.extensions().get::<crate::extension::EnterTime>().map(crate::extension::EnterTime::elapsed), "start a backend request");
    x_forwarded_for(&mut req)?;
    let mut client = get_client();
    let response = if req.headers().get(UPGRADE).is_some_and(|upgrade| upgrade.as_bytes().eq_ignore_ascii_case(b"websocket")) {
        // dump request
        let (part, body) = req.into_parts();
        let body = body.dump().await?;
        let req = Request::from_parts(part, body);

        // forward request
        let resp = client.request(req.clone()).await;

        // dump response
        let (part, body) = resp.into_parts();
        let body = body.dump().await?;
        let resp = Response::from_parts(part, body);

        let req_for_upgrade = req.clone();
        let resp_for_upgrade = resp.clone();

        // create forward task
        tokio::task::spawn(async move {
            // update both side
            let (s, c) = futures_util::join!(hyper::upgrade::on(req_for_upgrade), hyper::upgrade::on(resp_for_upgrade));
            let upgrade_as_server = s?;
            let upgrade_as_client = c?;
            // start a websocket forward
            ws_client_service::tcp_transfer(upgrade_as_server, upgrade_as_client).await?;
            <Result<(), BoxError>>::Ok(())
        });
        // return response to client
        resp
    } else {
        client.request(req).await
    };
    Ok(response)
}

#[instrument]
pub async fn http_backend_service(req: Request<SgBody>) -> Result<Response<SgBody>, Infallible> {
    match http_backend_service_inner(req).await {
        Ok(resp) => Ok(resp),
        Err(err) => Ok(Response::with_code_message(StatusCode::BAD_GATEWAY, format!("[Sg.Client] Client error: {err}"))),
    }
}

#[inline]
pub fn get_http_backend_service() -> ArcHyperService {
    ArcHyperService::new(hyper::service::service_fn(http_backend_service))
}

#[cold]
#[inline]
pub fn get_echo_service() -> ArcHyperService {
    ArcHyperService::new(hyper::service::service_fn(echo::echo))
}
