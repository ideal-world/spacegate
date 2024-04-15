use std::{net::SocketAddr, str::FromStr, time::Duration};

use spacegate_kernel::{
    layers::{
        gateway,
        http_route::{match_request::SgHttpPathMatch, SgHttpBackendLayer, SgHttpRoute, SgHttpRouteRuleLayer},
    },
    listener::SgListen,
    service::get_http_backend_service,
};
use tokio_rustls::rustls::ServerConfig;
use tokio_util::sync::CancellationToken;
use tower_layer::Layer;
#[tokio::test]
async fn test_https() {
    tokio::spawn(gateway());
    tokio::spawn(axum_server());
    // wait for startup
    tokio::time::sleep(Duration::from_millis(200)).await;
    let client = reqwest::Client::builder().danger_accept_invalid_certs(true).build().unwrap();
    let echo = client.post("https://[::]:9002/tls/echo").body("1").send().await.expect("fail to send").text().await.expect("fail to get text");
    println!("echo: {}", echo);
    let echo = client.get("https://[::]:9002/baidu").send().await.expect("fail to send").text().await.expect("fail to get text");
    println!("echo: {}", echo);
    let echo = client.post("http://[::]:9002/tls/echo").body("1").send().await.expect_err("should be error");
    println!("echo: {}", echo);
}

async fn gateway() {
    let cancel = CancellationToken::default();
    let gateway = gateway::SgGatewayLayer::builder("test_multi_part")
        .http_routers([(
            "test_upload".to_string(),
            SgHttpRoute::builder()
                .rule(
                    SgHttpRouteRuleLayer::builder()
                        .match_item(SgHttpPathMatch::Prefix("/tls".into()))
                        .backend(SgHttpBackendLayer::builder().host("[::]").port(9003).build())
                        .build(),
                )
                .rule(
                    SgHttpRouteRuleLayer::builder()
                        .match_item(SgHttpPathMatch::Prefix("/baidu".into()))
                        .backend(SgHttpBackendLayer::builder().protocol("https").host("www.baidu.com").port(443).build())
                        .build(),
                )
                .build(),
        )])
        .build();
    let addr = SocketAddr::from_str("[::]:9002").expect("invalid host");
    let cert = include_bytes!("test_https/.cert");
    let key = include_bytes!("test_https/.key");
    let tls_config = ServerConfig::builder()
        .with_no_client_auth()
        .with_single_cert(
            rustls_pemfile::certs(&mut cert.as_slice()).filter_map(Result::ok).collect(),
            rustls_pemfile::private_key(&mut key.as_slice()).ok().flatten().expect("fail to get key"),
        )
        .expect("fail to build tls config");
    let listener = SgListen::new(addr, gateway.layer(get_http_backend_service()), cancel, "listener").with_tls_config(tls_config);
    listener.listen().await.expect("fail to listen");
}

async fn axum_server() {
    use axum::{response::IntoResponse, serve, Router};
    pub async fn echo(text: String) -> impl IntoResponse {
        text
    }
    serve(
        tokio::net::TcpListener::bind("[::]:9003").await.expect("fail to bind"),
        Router::new().route("/tls/echo", axum::routing::get(echo)),
    )
    .await
    .expect("fail to serve");
}
