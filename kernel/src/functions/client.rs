use crate::{
    config::{gateway_dto::SgProtocol, http_route_dto::SgHttpBackendRef},
    plugins::filters::SgRouteFilterContext,
};
use http::Request;
use hyper::{client::HttpConnector, Body, Client, Error};
use hyper_rustls::{ConfigBuilderExt, HttpsConnector};
use tardis::basic::{error::TardisError, result::TardisResult};

pub fn init() -> TardisResult<Client<HttpsConnector<HttpConnector>>> {
    // TODO timeout
    let tls_cfg = rustls::ClientConfig::builder().with_safe_defaults().with_native_roots().with_no_client_auth();
    let https = hyper_rustls::HttpsConnectorBuilder::new().with_tls_config(tls_cfg).https_or_http().enable_http1().build();
    let tls_client = Client::builder().build(https);
    Ok(tls_client)
}

pub async fn request(client: &Client<HttpsConnector<HttpConnector>>, backend: Option<&SgHttpBackendRef>, mut ctx: SgRouteFilterContext) -> TardisResult<SgRouteFilterContext> {
    let mut req = Request::builder();
    req = req.method(ctx.get_req_method().clone());
    for (k, v) in ctx.get_req_headers() {
        req = req.header(k.as_str(), v.to_str().unwrap());
    }
    if let Some(backend) = backend {
        let url = format!(
            "{}://{}:{}/{}",
            backend.protocol.as_ref().unwrap_or(&SgProtocol::Http),
            backend.namespace_or_host.as_ref().unwrap_or(&"default".to_string()),
            backend.port,
            backend.name_or_path
        );
        req = req.uri(url);
    } else {
        req = req.uri(ctx.get_req_uri().clone());
    }
    let req =
        req.body(ctx.pop_req_body_raw()?.unwrap_or_else(|| Body::empty())).map_err(|error| TardisError::internal_error(&format!("[SG.Route] Build request error:{error}"), ""))?;
    let response = client.request(req).await.map_err(|error: Error| TardisError::internal_error(&format!("[SG.Client] Request error: {error}"), ""))?;
    ctx = ctx.resp(response.status(), response.headers().clone(), response.into_body());
    Ok(ctx)
}
