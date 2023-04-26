use std::time::Duration;

use crate::{
    config::{gateway_dto::SgProtocol, http_route_dto::SgHttpBackendRef},
    plugins::filters::SgRouteFilterContext,
};
use http::Request;
use hyper::{client::HttpConnector, Body, Client, Error};
use hyper_rustls::{ConfigBuilderExt, HttpsConnector};
use tardis::{
    basic::{error::TardisError, result::TardisResult},
    tokio::time::timeout,
};

const DEFAULT_TIMEOUT_MS: u64 = 5000;

pub fn init() -> TardisResult<Client<HttpsConnector<HttpConnector>>> {
    // TODO timeout
    let tls_cfg = rustls::ClientConfig::builder().with_safe_defaults().with_native_roots().with_no_client_auth();
    let https = hyper_rustls::HttpsConnectorBuilder::new().with_tls_config(tls_cfg).https_or_http().enable_http1().build();
    let tls_client = Client::builder().build(https);
    Ok(tls_client)
}

pub async fn request(
    client: &Client<HttpsConnector<HttpConnector>>,
    backend: Option<&SgHttpBackendRef>,
    rule_timeout_ms: Option<u64>,
    redirect: bool,
    mut ctx: SgRouteFilterContext,
) -> TardisResult<SgRouteFilterContext> {
    if redirect {
        ctx = do_request(client, &ctx.get_req_uri().to_string(), rule_timeout_ms, ctx).await?;
    }
    if let Some(backend) = backend {
        let host = format!("{}{}", backend.namespace.as_ref().map(|n| format!("{n}.")).unwrap_or("".to_string()), backend.name_or_host);
        let url = format!(
            "{}://{}:{}/{}",
            backend.protocol.as_ref().unwrap_or(&SgProtocol::Http),
            host,
            backend.port,
            ctx.get_req_uri().path_and_query().map(|p| p.as_str()).unwrap_or("")
        );
        let timeout_ms = if let Some(timeout) = backend.timeout { Some(timeout) } else { rule_timeout_ms };
        ctx = do_request(client, &url, timeout_ms, ctx).await?;
    }
    Ok(ctx)
}

async fn do_request(client: &Client<HttpsConnector<HttpConnector>>, url: &str, timeout_ms: Option<u64>, mut ctx: SgRouteFilterContext) -> TardisResult<SgRouteFilterContext> {
    let timeout_ms = timeout_ms.unwrap_or(DEFAULT_TIMEOUT_MS);

    let mut req = Request::builder();
    req = req.method(ctx.get_req_method().clone());
    for (k, v) in ctx.get_req_headers() {
        req = req.header(k.as_str(), v.to_str().unwrap());
    }
    req = req.uri(url);
    let req = req.body(ctx.pop_req_body_raw()?.unwrap_or_else(Body::empty)).map_err(|error| TardisError::internal_error(&format!("[SG.Route] Build request error:{error}"), ""))?;
    let response = match timeout(Duration::from_millis(timeout_ms), client.request(req)).await {
        Ok(response) => response.map_err(|error: Error| TardisError::internal_error(&format!("[SG.Client] Request error: {error}"), "")),
        Err(_) => Err(TardisError::internal_error(&format!("[SG.Client] Request error: timeout for {timeout_ms} ms"), "")),
    }?;
    ctx = ctx.resp(response.status(), response.headers().clone(), response.into_body());
    Ok(ctx)
}

#[cfg(test)]
mod tests {
    use http::{HeaderMap, Method, Uri, Version};
    use hyper::Body;
    use tardis::tokio;

    use crate::{
        config::http_route_dto::SgHttpBackendRef,
        functions::client::{init, request},
        plugins::filters::SgRouteFilterContext,
    };

    #[tokio::test]
    async fn test_request() {
        let client = init().unwrap();
        assert!(request(
            &client,
            Some(&SgHttpBackendRef {
                name_or_host: "".to_string(),
                namespace: Some("www.baidu.com".to_string()),
                port: 80,
                ..Default::default()
            }),
            None,
            false,
            SgRouteFilterContext::new(
                Method::GET,
                Uri::from_static("http://sg.idealworld.group"),
                Version::HTTP_11,
                HeaderMap::new(),
                Body::empty(),
                "127.0.0.1:8080".parse().unwrap(),
                "".to_string(),
            )
        )
        .await
        .is_ok());

        assert!(request(
            &client,
            Some(&SgHttpBackendRef {
                name_or_host: "anything".to_string(),
                namespace: Some("httpbin.org".to_string()),
                port: 80,
                ..Default::default()
            }),
            Some(20000),
            false,
            SgRouteFilterContext::new(
                Method::GET,
                Uri::from_static("http://sg.idealworld.group"),
                Version::HTTP_11,
                HeaderMap::new(),
                Body::empty(),
                "127.0.0.1:8080".parse().unwrap(),
                "".to_string(),
            )
        )
        .await
        .is_ok());

        assert!(request(
            &client,
            Some(&SgHttpBackendRef {
                name_or_host: "anything".to_string(),
                namespace: Some("httpbin.org".to_string()),
                port: 80,
                ..Default::default()
            }),
            Some(10),
            false,
            SgRouteFilterContext::new(
                Method::GET,
                Uri::from_static("http://sg.idealworld.group"),
                Version::HTTP_11,
                HeaderMap::new(),
                Body::empty(),
                "127.0.0.1:8080".parse().unwrap(),
                "".to_string(),
            )
        )
        .await
        .is_err());

        assert!(request(
            &client,
            Some(&SgHttpBackendRef {
                name_or_host: "anything".to_string(),
                namespace: Some("httpbin.org".to_string()),
                port: 80,
                timeout: Some(20000),
                ..Default::default()
            }),
            Some(10),
            false,
            SgRouteFilterContext::new(
                Method::GET,
                Uri::from_static("http://sg.idealworld.group"),
                Version::HTTP_11,
                HeaderMap::new(),
                Body::empty(),
                "127.0.0.1:8080".parse().unwrap(),
                "".to_string(),
            )
        )
        .await
        .is_ok());

        assert!(request(
            &client,
            None,
            Some(20000),
            true,
            SgRouteFilterContext::new(
                Method::GET,
                Uri::from_static("https://httpbin.org/anything"),
                Version::HTTP_11,
                HeaderMap::new(),
                Body::empty(),
                "127.0.0.1:8080".parse().unwrap(),
                "".to_string(),
            )
        )
        .await
        .is_ok());
    }
}
