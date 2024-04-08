use std::convert::Infallible;
use std::sync::Arc;

use futures_util::future::BoxFuture;
use futures_util::Future;
use hyper::{header::UPGRADE, Request, Response, StatusCode};
use tracing::instrument;

use crate::helper_layers::map_future::MapFuture;
use crate::service::http_client_service::get_client;
use crate::utils::x_forwarded_for;
use crate::BoxError;
use crate::SgBody;
use crate::SgResponse;
use crate::SgResponseExt;

pub mod echo;
pub mod http_client_service;

pub mod ws_client_service;
pub trait CloneHyperService<R>: hyper::service::Service<R> {
    fn clone_box(&self) -> Box<dyn CloneHyperService<R, Response = Self::Response, Error = Self::Error, Future = Self::Future> + Send + Sync>;
}

impl<R, T> CloneHyperService<R> for T
where
    T: hyper::service::Service<R> + Send + Sync + Clone + 'static,
{
    fn clone_box(&self) -> Box<dyn CloneHyperService<R, Response = T::Response, Error = T::Error, Future = T::Future> + Send + Sync> {
        Box::new(self.clone())
    }
}
pub struct ArcHyperService {
    pub boxed: Arc<
        dyn CloneHyperService<Request<SgBody>, Response = Response<SgBody>, Error = Infallible, Future = BoxFuture<'static, Result<Response<SgBody>, Infallible>>> + Send + Sync,
    >,
}

impl std::fmt::Debug for ArcHyperService {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ArcHyperService").finish()
    }
}

impl Clone for ArcHyperService {
    fn clone(&self) -> Self {
        Self { boxed: self.boxed.clone() }
    }
}

impl ArcHyperService {
    pub fn new<T>(service: T) -> Self
    where
        T: Clone + CloneHyperService<Request<SgBody>, Response = Response<SgBody>, Error = Infallible> + Send + Sync + 'static,
        T::Future: Future<Output = Result<Response<SgBody>, Infallible>> + 'static + Send,
    {
        let map_fut = MapFuture::new(service, |fut| Box::pin(fut) as _);
        Self { boxed: Arc::new(map_fut) }
    }
}

impl hyper::service::Service<Request<SgBody>> for ArcHyperService {
    type Response = Response<SgBody>;
    type Error = Infallible;
    type Future = BoxFuture<'static, Result<Self::Response, Self::Error>>;

    fn call(&self, req: Request<SgBody>) -> Self::Future {
        Box::pin(self.boxed.call(req))
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
    let mut response = if req.headers().get(UPGRADE).is_some_and(|upgrade| upgrade.as_bytes().eq_ignore_ascii_case(b"websocket")) {
        // we only support websocket upgrade now
        // if !upgrade.as_bytes().eq_ignore_ascii_case(b"websocket") {
        //     return Ok(Response::with_code_message(StatusCode::NOT_IMPLEMENTED, "[Sg.Websocket] unsupported upgrade protocol"));
        // }
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
            ws_client_service::service(upgrade_as_server, upgrade_as_client).await?;
            <Result<(), BoxError>>::Ok(())
        });
        tracing::trace!(elapsed = ?resp.extensions().get::<crate::extension::EnterTime>().map(crate::extension::EnterTime::elapsed), "finish backend websocket forward");
        // return response to client
        resp
    } else {
        let resp = client.request(req).await;
        tracing::trace!(elapsed = ?resp.extensions().get::<crate::extension::EnterTime>().map(crate::extension::EnterTime::elapsed), "finish backend request");
        resp
    };
    response.extensions_mut().insert(unsafe { crate::extension::FromBackend::new() });
    Ok(response)
}

#[instrument]
pub async fn http_backend_service(req: Request<SgBody>) -> Result<Response<SgBody>, Infallible> {
    match http_backend_service_inner(req).await {
        Ok(resp) => Ok(resp),
        Err(err) => Ok(Response::with_code_message(StatusCode::BAD_GATEWAY, format!("[Sg.Client] Client error: {err}"))),
    }
}

pub fn get_http_backend_service() -> ArcHyperService {
    ArcHyperService::new(hyper::service::service_fn(http_backend_service))
}

pub fn get_echo_service() -> ArcHyperService {
    ArcHyperService::new(hyper::service::service_fn(echo::echo))
}
