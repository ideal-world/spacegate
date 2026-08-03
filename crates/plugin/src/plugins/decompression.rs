//! This layer is used to make response's encoding compatible with the request's accept encoding.
//!
//! see also:
//! - https://developer.mozilla.org/zh-CN/docs/Web/HTTP/Headers/Accept-Encoding
//! - https://developer.mozilla.org/zh-CN/docs/Web/HTTP/Headers/Content-Encoding
//!
//!

use hyper::{Request, Response};
use serde::{Deserialize, Serialize};
use spacegate_kernel::{helper_layers::function::Inner, BoxError, SgBody};
use tower::{service_fn as tower_service_fn, ServiceExt};
use tower_http::decompression::Decompression as TowerDecompression;

use crate::Plugin;

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[cfg_attr(feature = "schema", schemars(title = "解压插件配置"))]
#[serde(default)]
pub struct DecompressionConfig {}

#[derive(Debug, Clone, Default)]
/// 对上游压缩响应进行通用解压的插件实例。
pub struct DecompressionPlugin;

impl Plugin for DecompressionPlugin {
    const CODE: &'static str = "decompression";

    async fn call(&self, req: Request<SgBody>, inner: Inner) -> Result<Response<SgBody>, BoxError> {
        // 将 Hyper 内层服务桥接为 Tower service，再由 tower-http 解压响应流。
        let service = tower_service_fn(move |request| {
            let inner = inner.clone();
            async move { Ok::<_, std::convert::Infallible>(inner.call(request).await) }
        });
        let response = TowerDecompression::new(service).oneshot(req).await.expect("SpaceGate inner service is infallible");
        Ok(response.map(SgBody::new))
    }

    fn create(config: crate::PluginConfig) -> Result<Self, BoxError> {
        let _: DecompressionConfig = serde_json::from_value(config.spec)?;
        Ok(Self)
    }

    #[cfg(feature = "schema")]
    fn schema_opt() -> Option<schemars::schema::RootSchema> {
        use crate::PluginSchemaExt;
        Some(Self::schema())
    }
}

#[cfg(feature = "schema")]
crate::schema!(DecompressionPlugin, DecompressionConfig);

#[cfg(test)]
mod test {
    use super::*;
    use hyper::header::{self, CONTENT_ENCODING};
    use spacegate_kernel::ArcHyperService;
    use tokio::io::AsyncWriteExt;

    async fn compress(req: Request<SgBody>) -> Result<Response<SgBody>, std::convert::Infallible> {
        let body_data = req.into_body().dump().await.expect("dump body").get_dumped().expect("get dumped").clone();
        let mut encoder = async_compression::tokio::write::GzipEncoder::new(Vec::new());
        encoder.write_all(body_data.as_ref()).await.expect("fail to write");
        encoder.shutdown().await.expect("fail to write");
        let x = encoder.into_inner();
        let resp = Response::builder().header(CONTENT_ENCODING, "gzip").body(SgBody::full(x)).expect("invalid response");
        Ok(resp)
    }

    /// 返回 Brotli 压缩响应，用于验证浏览器常见协商编码也能被通用层解压。
    async fn compress_brotli(req: Request<SgBody>) -> Result<Response<SgBody>, std::convert::Infallible> {
        let body_data = req.into_body().dump().await.expect("dump body").get_dumped().expect("get dumped").clone();
        let mut encoder = async_compression::tokio::write::BrotliEncoder::new(Vec::new());
        encoder.write_all(body_data.as_ref()).await.expect("fail to write");
        encoder.shutdown().await.expect("fail to write");
        let resp = Response::builder().header(CONTENT_ENCODING, "br").body(SgBody::full(encoder.into_inner())).expect("invalid response");
        Ok(resp)
    }

    #[tokio::test]
    async fn test_compress_decompress() {
        let plugin = DecompressionPlugin::create_by_spec(serde_json::json!({}), String::from("test").into()).expect("valid config");
        let message = "hello from spacegate";
        let req = Request::builder().header(header::ACCEPT_ENCODING, "gzip").body(SgBody::full(message)).expect("invalid req");
        let inner = Inner::new(ArcHyperService::new(hyper::service::service_fn(compress)));
        let resp = plugin.call(req, inner).await.expect("call plugin");
        let body = resp.into_body().dump().await.expect("dump body").get_dumped().expect("get dumped").clone();
        let s = std::str::from_utf8(body.as_ref()).expect("fail to parse");
        assert_eq!(s, message);
    }

    /// Brotli 响应必须在传给 HAI observe 前恢复为原始字节。
    #[tokio::test]
    async fn test_brotli_decompress() {
        let plugin = DecompressionPlugin::create_by_spec(serde_json::json!({}), String::from("test").into()).expect("valid config");
        let message = "hello from spacegate";
        let req = Request::builder().header(header::ACCEPT_ENCODING, "br").body(SgBody::full(message)).expect("invalid req");
        let inner = Inner::new(ArcHyperService::new(hyper::service::service_fn(compress_brotli)));
        let resp = plugin.call(req, inner).await.expect("call plugin");
        let body = resp.into_body().dump().await.expect("dump body").get_dumped().expect("get dumped").clone();
        let s = std::str::from_utf8(body.as_ref()).expect("fail to parse");
        assert_eq!(s, message);
    }
}
