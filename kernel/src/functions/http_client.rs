use std::time::Duration;

use crate::{config::gateway_dto::SgProtocol, plugins::context::SgRoutePluginContext};
use http::{HeaderMap, HeaderValue, Method, Request, Response, StatusCode};
use hyper::{client::HttpConnector, Body, Client, Error};
use hyper_rustls::{ConfigBuilderExt, HttpsConnector};
use tardis::{
    basic::{error::TardisError, result::TardisResult},
    log,
    tokio::time::timeout,
};

use super::http_route::SgBackend;

const DEFAULT_TIMEOUT_MS: u64 = 5000;

static mut DEFAULT_CLIENT: Option<Client<HttpsConnector<HttpConnector>>> = None;

pub fn init() -> TardisResult<Client<HttpsConnector<HttpConnector>>> {
    unsafe {
        if DEFAULT_CLIENT.is_none() {
            DEFAULT_CLIENT = Some(do_init()?);
        }
    }
    do_init()
}

fn do_init() -> TardisResult<Client<HttpsConnector<HttpConnector>>> {
    let tls_cfg = rustls::ClientConfig::builder().with_safe_defaults().with_native_roots().with_no_client_auth();
    let https = hyper_rustls::HttpsConnectorBuilder::new().with_tls_config(tls_cfg).https_or_http().enable_http1().build();
    let tls_client = Client::builder().build(https);
    Ok(tls_client)
}

fn default_client() -> &'static Client<HttpsConnector<HttpConnector>> {
    unsafe { DEFAULT_CLIENT.as_ref().expect("DEFAULT_CLIENT not initialized") }
}

pub async fn request(
    client: &Client<HttpsConnector<HttpConnector>>,
    backend: Option<&SgBackend>,
    rule_timeout_ms: Option<u64>,
    redirect: bool,
    mut ctx: SgRoutePluginContext,
) -> TardisResult<SgRoutePluginContext> {
    if redirect {
        ctx = do_request(client, &ctx.get_req_uri().to_string(), rule_timeout_ms, ctx).await?;
    }
    if let Some(backend) = backend {
        let scheme = backend.protocol.as_ref().unwrap_or(&SgProtocol::Http);
        let host = format!("{}{}", backend.name_or_host, backend.namespace.as_ref().map(|n| format!(".{n}")).unwrap_or("".to_string()));
        let port = if (backend.port == 0 || backend.port == 80) && scheme == &SgProtocol::Http || (backend.port == 0 || backend.port == 443) && scheme == &SgProtocol::Https {
            "".to_string()
        } else {
            format!(":{}", backend.port)
        };
        let url = format!("{}://{}{}{}", scheme, host, port, ctx.get_req_uri().path_and_query().map(|p| p.as_str()).unwrap_or(""));
        let timeout_ms = if let Some(timeout_ms) = backend.timeout_ms { Some(timeout_ms) } else { rule_timeout_ms };
        ctx = do_request(client, &url, timeout_ms, ctx).await?;
        ctx.set_chose_backend(backend);
    }
    Ok(ctx)
}

async fn do_request(client: &Client<HttpsConnector<HttpConnector>>, url: &str, timeout_ms: Option<u64>, mut ctx: SgRoutePluginContext) -> TardisResult<SgRoutePluginContext> {
    let ctx = match raw_request(Some(client), ctx.get_req_method().clone(), url, ctx.pop_req_body_raw()?, ctx.get_req_headers(), timeout_ms).await {
        Ok(response) => ctx.resp(response.status(), response.headers().clone(), response.into_body()),
        Err(e) => ctx.resp_from_error(e),
    };
    Ok(ctx)
}

pub async fn raw_request(
    client: Option<&Client<HttpsConnector<HttpConnector>>>,
    method: Method,
    url: &str,
    body: Option<Body>,
    headers: &HeaderMap<HeaderValue>,
    timeout_ms: Option<u64>,
) -> TardisResult<Response<Body>> {
    let timeout_ms = timeout_ms.unwrap_or(DEFAULT_TIMEOUT_MS);
    let method_str = method.to_string();
    let url_str = url.to_string();
    log::trace!("[SG.Client] Request method {} url {} , timeout {} ms", method_str, url_str, timeout_ms);

    let mut req = Request::builder();
    req = req.method(method);
    for (k, v) in headers {
        req = req.header(
            k.as_str(),
            v.to_str().map_err(|_| TardisError::bad_request(&format!("Header {} value is illegal: is not ascii", k), ""))?,
        );
    }
    req = req.uri(url);
    let req = req
        .body(body.unwrap_or_else(Body::empty))
        .map_err(|error| TardisError::internal_error(&format!("[SG.Route] Build request method {method_str} url {url_str} error:{error}"), ""))?;
    let req = if let Some(client) = client {
        client.request(req)
    } else {
        default_client().request(req)
    };
    let response = match timeout(Duration::from_millis(timeout_ms), req).await {
        Ok(response) => response.map_err(|error: Error| TardisError::internal_error(&format!("[SG.Client] Request method {method_str} url {url_str} error: {error}"), "")),
        Err(_) => {
            Response::builder().status(StatusCode::GATEWAY_TIMEOUT).body(Body::empty()).map_err(|e| TardisError::internal_error(&format!("[SG.Client] timeout error: {e}"), ""))
        }
    }?;
    Ok(response)
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use http::{HeaderMap, Method, Uri, Version};
    use hyper::Body;
    use tardis::{basic::result::TardisResult, tokio};

    use crate::{
        config::gateway_dto::SgProtocol,
        functions::{
            http_client::{init, request},
            http_route::SgBackend,
        },
        plugins::context::SgRoutePluginContext,
    };

    #[tokio::test]
    async fn test_request() -> TardisResult<()> {
        let client = init().unwrap();

        // test simple
        let mut resp = request(
            &client,
            Some(&SgBackend {
                name_or_host: "www.baidu.com".to_string(),
                port: 80,
                ..Default::default()
            }),
            None,
            false,
            SgRoutePluginContext::new_http(
                Method::GET,
                Uri::from_static("http://sg.idealworld.group"),
                Version::HTTP_11,
                HeaderMap::new(),
                Body::empty(),
                "127.0.0.1:8080".parse().unwrap(),
                "".to_string(),
                None,
            ),
        )
        .await?;
        assert_eq!(resp.get_resp_status_code().as_u16(), 200);
        let body = String::from_utf8(resp.pop_resp_body().await?.unwrap()).unwrap();
        assert!(body.contains("百度一下"));

        // test get
        let mut resp = request(
            &client,
            Some(&SgBackend {
                name_or_host: "postman-echo.com".to_string(),
                port: 80,
                ..Default::default()
            }),
            Some(20000),
            false,
            SgRoutePluginContext::new_http(
                Method::GET,
                Uri::from_static("http://sg.idealworld.group/get?foo1=bar1&foo2=bar2"),
                Version::HTTP_11,
                HeaderMap::new(),
                Body::empty(),
                "127.0.0.1:8080".parse().unwrap(),
                "".to_string(),
                None,
            ),
        )
        .await?;
        assert_eq!(resp.get_resp_status_code().as_u16(), 200);
        let body = String::from_utf8(resp.pop_resp_body().await?.unwrap()).unwrap();
        assert!(body.contains(r#""url": "http://postman-echo.com/get?foo1=bar1&foo2=bar2""#));

        // test post with tls
        let mut resp = request(
            &client,
            Some(&SgBackend {
                name_or_host: "postman-echo.com".to_string(),
                protocol: Some(SgProtocol::Https),
                port: 443,
                ..Default::default()
            }),
            Some(20000),
            false,
            SgRoutePluginContext::new_http(
                Method::POST,
                Uri::from_static("http://sg.idealworld.group/post?foo1=bar1&foo2=bar2"),
                Version::HTTP_11,
                HeaderMap::new(),
                Body::from("星航".as_bytes()),
                "127.0.0.1:8080".parse().unwrap(),
                "".to_string(),
                None,
            ),
        )
        .await?;
        assert_eq!(resp.get_resp_status_code().as_u16(), 200);
        let body = String::from_utf8(resp.pop_resp_body().await?.unwrap()).unwrap();
        assert!(body.contains(r#""url": "https://postman-echo.com/post?foo1=bar1&foo2=bar2""#));
        assert!(body.contains(r#""data": "星航""#));

        // test timeout
        let mut resp = request(
            &client,
            Some(&SgBackend {
                name_or_host: "postman-echo.com".to_string(),
                port: 80,
                ..Default::default()
            }),
            Some(5),
            false,
            SgRoutePluginContext::new_http(
                Method::GET,
                Uri::from_static("http://sg.idealworld.group/get?foo1=bar1&foo2=bar2"),
                Version::HTTP_11,
                HeaderMap::new(),
                Body::empty(),
                "127.0.0.1:8080".parse().unwrap(),
                "".to_string(),
                None,
            ),
        )
        .await
        .unwrap();
        assert_eq!(resp.get_resp_status_code().as_u16(), 504);

        let mut resp = request(
            &client,
            Some(&SgBackend {
                name_or_host: "postman-echo.com".to_string(),
                port: 80,
                timeout_ms: Some(20000),
                ..Default::default()
            }),
            Some(20000),
            false,
            SgRoutePluginContext::new_http(
                Method::GET,
                Uri::from_static("http://sg.idealworld.group/get?foo1=bar1&foo2=bar2"),
                Version::HTTP_11,
                HeaderMap::new(),
                Body::empty(),
                "127.0.0.1:8080".parse().unwrap(),
                "".to_string(),
                None,
            ),
        )
        .await?;
        assert_eq!(resp.get_resp_status_code().as_u16(), 200);
        let body = String::from_utf8(resp.pop_resp_body().await?.unwrap()).unwrap();
        assert!(body.contains(r#""url": "http://postman-echo.com/get?foo1=bar1&foo2=bar2""#));

        // test redirect
        let mut resp = request(
            &client,
            None,
            Some(20000),
            true,
            SgRoutePluginContext::new_http(
                Method::GET,
                Uri::from_static("http://postman-echo.com/get?foo1=bar1&foo2=bar2"),
                Version::HTTP_11,
                HeaderMap::new(),
                Body::empty(),
                "127.0.0.1:8080".parse().unwrap(),
                "".to_string(),
                None,
            ),
        )
        .await
        .unwrap();
        assert_eq!(resp.get_resp_status_code().as_u16(), 200);
        let body = String::from_utf8(resp.pop_resp_body().await?.unwrap()).unwrap();
        assert!(body.contains(r#""url": "http://postman-echo.com/get?foo1=bar1&foo2=bar2""#));

        Ok(())
    }
}
