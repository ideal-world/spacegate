use std::{
    net::{Ipv6Addr, SocketAddr, SocketAddrV6},
    str::FromStr,
    sync::Arc,
    time::Duration,
};

use axum_server::tls_rustls::RustlsConfig;
use hyper::{client, Request};
use spacegate_kernel::{
    backend_service::{get_http_backend_service, http_backend_service},
    listener::SgListen,
    service::{
        gateway,
        http_route::{match_request::HttpPathMatchRewrite, HttpBackend, HttpRoute, HttpRouteRule},
    },
    SgBody,
};
use tokio_rustls::rustls::ServerConfig;
use tokio_util::sync::CancellationToken;
use tower_layer::Layer;
#[tokio::test]
async fn test_h2_over_tls() {
    std::env::set_var("RUST_LOG", "TRACE,h2=off,tokio_util=off,spacegate_kernel=TRACE");
    tracing_subscriber::fmt().with_env_filter(tracing_subscriber::EnvFilter::from_default_env()).init();
    tokio::spawn(gateway());
    tokio::spawn(axum_server());
    // wait for startup
    tokio::time::sleep(Duration::from_millis(200)).await;
    let client = reqwest::Client::builder().danger_accept_invalid_certs(true).http2_prior_knowledge().build().unwrap();
    let mut task_set = tokio::task::JoinSet::new();
    for idx in 0..1 {
        let client = client.clone();
        task_set.spawn(async move {
            let echo = client.post("https://[::]:9443/echo").body(idx.to_string()).send().await.expect("fail to send").text().await.expect("fail to get text");
            println!("echo: {echo}");
            assert_eq!(idx.to_string(), echo);
        });
    }
    while let Some(Ok(r)) = task_set.join_next().await {}
}

#[tokio::test]
async fn test_h2c() {
    std::env::set_var("RUST_LOG", "TRACE,h2=off,tokio_util=off,spacegate_kernel=TRACE");
    tracing_subscriber::fmt().with_env_filter(tracing_subscriber::EnvFilter::from_default_env()).init();
    tokio::spawn(gateway());
    tokio::spawn(axum_server());
    // wait for startup
    tokio::time::sleep(Duration::from_millis(200)).await;
    let client = reqwest::Client::builder().danger_accept_invalid_certs(true).http2_prior_knowledge().build().unwrap();
    let mut task_set = tokio::task::JoinSet::new();
    for idx in 0..10 {
        let client = client.clone();
        task_set.spawn(async move {
            let echo = client.post("http://[::]:9080/echo").body(idx.to_string()).send().await.expect("fail to send").text().await.expect("fail to get text");
            println!("echo: {echo}");
            assert_eq!(idx.to_string(), echo);
        });
    }
    while let Some(Ok(r)) = task_set.join_next().await {}
}

async fn gateway() {
    let cancel = CancellationToken::default();
    let gateway = gateway::Gateway::builder("test_h2")
        .http_routers([(
            "test_h2".to_string(),
            HttpRoute::builder().rule(HttpRouteRule::builder().match_all().backend(HttpBackend::builder().host("[::]").port(9003).schema("https").build()).build()).build(),
        )])
        .build();
    let addr = SocketAddr::from_str("[::]:9080").expect("invalid host");
    let addr_tls = SocketAddr::from_str("[::]:9443").expect("invalid host");

    let listener_tls = SgListen::new(addr_tls, gateway.as_service(), cancel.clone(), "listener").with_tls_config(tls_config());
    let listener = SgListen::new(addr, gateway.as_service(), cancel, "listener");

    let f_tls = listener_tls.listen();
    let f = listener.listen();
    let (res_tls, res) = tokio::join!(f_tls, f);
    res_tls.expect("fail to listen tls");
    res.expect("fail to listen");
}

const CERT: &[u8] = include_bytes!("test_https/.cert");
const KEY: &[u8] = include_bytes!("test_https/.key");
fn tls_config() -> ServerConfig {
    let mut config = ServerConfig::builder()
        .with_no_client_auth()
        .with_single_cert(
            rustls_pemfile::certs(&mut CERT).filter_map(Result::ok).collect(),
            rustls_pemfile::private_key(&mut KEY).ok().flatten().expect("fail to get key"),
        )
        .expect("fail to build tls config");
    config.alpn_protocols = vec![b"h2".to_vec()];
    config
}

async fn axum_server() {
    use axum::{response::IntoResponse, serve, Router};
    pub async fn echo(request: axum::extract::Request<axum::body::Body>) -> impl IntoResponse {
        axum::response::Response::new(request.into_body())
    }
    let config = axum_server::tls_rustls::RustlsConfig::from_pem(CERT.to_vec(), KEY.to_vec()).await.expect("fail to build");
    axum_server::bind_rustls(SocketAddr::V6(SocketAddrV6::new(Ipv6Addr::UNSPECIFIED, 9003, 0, 0)), config)
        .serve(Router::new().route("/echo", axum::routing::post(echo)).into_make_service())
        .await
        .expect("fail to serve");
}
