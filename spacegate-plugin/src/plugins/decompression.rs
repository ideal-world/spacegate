//! This layer is used to make response's encoding compatible with the request's accept encoding.
//!
//! see also:
//! - https://developer.mozilla.org/zh-CN/docs/Web/HTTP/Headers/Accept-Encoding
//! - https://developer.mozilla.org/zh-CN/docs/Web/HTTP/Headers/Content-Encoding
//!
//!

use std::convert::Infallible;

use hyper::{Request, Response};
use serde::{Deserialize, Serialize};
use spacegate_tower::{SgBody, SgBoxService};
use tower::BoxError;
use tower_http::decompression::Decompression as TowerDecompression;
use tower_layer::Layer;
use tower_service::Service;

use crate::{def_plugin, MakeSgLayer};

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct DecompressionConfig {}

#[derive(Debug, Clone)]
pub struct DecompressionLayer;

impl DecompressionLayer {}

impl<S> Layer<S> for DecompressionLayer {
    type Service = Decompression<S>;

    fn layer(&self, inner: S) -> Self::Service {
        Decompression::new(inner)
    }
}

#[derive(Debug, Clone)]
pub struct Decompression<S> {
    inner: TowerDecompression<S>,
}

impl<S> Decompression<S> {
    pub fn new(inner: S) -> Self {
        let inner = TowerDecompression::new(inner);
        Self { inner }
    }
}

impl<S> Service<Request<SgBody>> for Decompression<S>
where
    S: Service<Request<SgBody>, Response = Response<SgBody>, Error = Infallible>,
    <S as Service<Request<SgBody>>>::Future: Send + 'static,
{
    type Response = Response<SgBody>;
    type Error = Infallible;
    type Future = <SgBoxService as Service<Request<SgBody>>>::Future;

    fn poll_ready(&mut self, cx: &mut std::task::Context<'_>) -> std::task::Poll<Result<(), Self::Error>> {
        self.inner.poll_ready(cx)
    }

    fn call(&mut self, req: Request<SgBody>) -> Self::Future {
        let fut = self.inner.call(req);
        Box::pin(async move {
            let response = fut.await.expect("infallible");
            Ok(response.map(SgBody::new_boxed_error))
        })
    }
}

impl MakeSgLayer for DecompressionConfig {
    fn make_layer(&self) -> Result<spacegate_tower::SgBoxLayer, BoxError> {
        let layer = DecompressionLayer {};
        Ok(spacegate_tower::SgBoxLayer::new(layer))
    }
}

def_plugin!("decompression", DecompressionPlugin, DecompressionConfig);

#[cfg(test)]
mod test {
    use super::*;
    use hyper::header::{self, CONTENT_ENCODING};
    use tardis::tokio::{self, io::AsyncWriteExt};
    use tower::{service_fn, ServiceExt};
    pub async fn compress(req: Request<SgBody>) -> Result<Response<SgBody>, Infallible> {
        let body_data = req.into_body().dump().await.expect("dump body").get_dumped().expect("get dumped").clone();
        let mut encoder = async_compression::tokio::write::GzipEncoder::new(Vec::new());
        encoder.write_all(body_data.as_ref()).await.expect("fail to write");
        encoder.shutdown().await.expect("fail to write");
        let x = encoder.into_inner();
        let resp = Response::builder().header(CONTENT_ENCODING, "gzip").body(SgBody::full(x)).expect("invalid response");
        Ok(resp)
    }

    #[tokio::test]
    async fn test_compress_decompress() {
        let mut service = Decompression::new(SgBoxService::new(service_fn(compress)));
        let message = "hello from spacegate";
        let req = Request::builder().header(header::ACCEPT_ENCODING, "gzip").body(SgBody::full(message)).expect("invalid req");
        let resp = service.ready().await.expect("fail to ready").call(req).await.expect("call service");
        let body = resp.into_body().dump().await.expect("dump body").get_dumped().expect("get dumped").clone();
        let s = std::str::from_utf8(body.as_ref()).expect("fail to parse");
        assert_eq!(s, message);
    }
}
