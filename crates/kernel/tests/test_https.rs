use std::{net::SocketAddr, str::FromStr, time::Duration};

use spacegate_kernel::{
    listener::SgListen,
    service::{
        gateway,
        http_route::{match_request::HttpPathMatchRewrite, HttpBackend, HttpRoute, HttpRouteRule},
    },
};
use tokio_rustls::rustls::ServerConfig;
use tokio_util::sync::CancellationToken;
#[tokio::test]
async fn test_https() {
    tokio::spawn(gateway());
    tokio::spawn(axum_server());
    // wait for startup
    tokio::time::sleep(Duration::from_millis(200)).await;
    let client = reqwest::Client::builder().danger_accept_invalid_certs(true).build().unwrap();
    let echo = client.post("https://[::]:9443/tls/echo").body("1").send().await.expect("fail to send").text().await.expect("fail to get text");
    println!("echo: {}", echo);
    let echo = client.get("https://[::]:9443/baidu").send().await.expect("fail to send").text().await.expect("fail to get text");
    println!("echo: {}", echo);
    let echo = client.post("http://[::]:9443/tls/echo").body("1").send().await.expect_err("should be error");
    eprintln!("echo: {}", echo);
    let echo = client.post("http://[::]:9080/tls/echo").body("1").send().await.expect("fail to send").text().await.expect("fail to get text");
    println!("echo: {}", echo);
}

async fn gateway() {
    let cancel = CancellationToken::default();
    let gateway = gateway::Gateway::builder("test_multi_part")
        .http_routers([(
            "test_upload".to_string(),
            HttpRoute::builder()
                .rule(HttpRouteRule::builder().match_item(HttpPathMatchRewrite::prefix("/tls")).backend(HttpBackend::builder().host("[::]").port(9003).build()).build())
                .rule(
                    HttpRouteRule::builder()
                        .match_item(HttpPathMatchRewrite::prefix("/baidu"))
                        .backend(HttpBackend::builder().schema("https").host("www.baidu.com").port(443).build())
                        .build(),
                )
                .build(),
        )])
        .build();
    let cert = include_bytes!("test_https/.cert");
    let key = include_bytes!("test_https/.key");
    let tls_config = ServerConfig::builder()
        .with_no_client_auth()
        .with_single_cert(
            rustls_pemfile::certs(&mut cert.as_slice()).filter_map(Result::ok).collect(),
            rustls_pemfile::private_key(&mut key.as_slice()).ok().flatten().expect("fail to get key"),
        )
        .expect("fail to build tls config");
    let http_listener = SgListen::new(
        SocketAddr::from_str("[::]:9080").expect("invalid host"),
        gateway.as_service(),
        cancel.child_token(),
    );
    let https_listener = SgListen::new(
        SocketAddr::from_str("[::]:9443").expect("invalid host"),
        gateway.as_service(),
        cancel.child_token(),
    )
    .with_tls_config(tls_config);
    let task = tokio::spawn(async move {
        http_listener.listen().await.expect("fail to listen");
    });
    let task_tls = tokio::spawn(async move {
        https_listener.listen().await.expect("fail to listen");
    });
    let (_, _) = tokio::join!(task, task_tls);
}

async fn axum_server() {
    use axum::{response::IntoResponse, serve, Router};
    pub async fn echo(text: String) -> impl IntoResponse {
        text
    }
    serve(
        tokio::net::TcpListener::bind("[::]:9003").await.expect("fail to bind"),
        Router::new().route("/tls/echo", axum::routing::post(echo)),
    )
    .await
    .expect("fail to serve");
}
