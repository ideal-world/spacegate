use std::{net::SocketAddr, str::FromStr, time::Duration};

use reqwest::{
    multipart::{Form, Part},
    Body,
};
use spacegate_kernel::{
    listener::SgListen,
    service::{
        http_gateway,
        http_route::{HttpBackend, HttpRoute, HttpRouteRule},
        Http,
    },
};
use tokio::fs::File;
use tokio_util::io::ReaderStream;
use tokio_util::sync::CancellationToken;
#[tokio::test]
async fn test_multi_part() {
    tokio::spawn(gateway());
    tokio::spawn(axum_server());
    // wait for startup
    tokio::time::sleep(Duration::from_millis(200)).await;
    let client = reqwest::Client::new();
    let file = File::open("./tests/test_multi_part.rs").await.expect("fail to open file");
    let form = Form::new().part("id", Part::text("hello")).part("file", Part::stream(Body::wrap_stream(ReaderStream::new(file))));
    let md5 = client.post("http://[::]:9002/md5").multipart(form).send().await.expect("fail to send").text().await.expect("fail to get text");
    println!("md5: {}", md5);
}

async fn gateway() {
    let cancel = CancellationToken::default();
    let gateway = http_gateway::Gateway::builder("test_multi_part")
        .http_routers([(
            "test_upload".to_string(),
            HttpRoute::builder().rule(HttpRouteRule::builder().match_all().backend(HttpBackend::builder().host("[::]").port(9003).build()).build()).build(),
        )])
        .build();
    let addr = SocketAddr::from_str("[::]:9002").expect("invalid host");
    let listener = SgListen::new(addr, cancel).with_service(Http::new(gateway.as_service()));

    listener.listen().await.expect("fail to listen");
}

async fn axum_server() {
    use axum::{extract::Multipart, response::IntoResponse, serve, Router};
    pub async fn md5(mut multipart: Multipart) -> impl IntoResponse {
        let mut md5_context = md5::Context::new();
        while let Some(field) = multipart.next_field().await.unwrap() {
            let bytes = field.bytes().await.expect("fail to load bytes");
            println!("read field with length: {}", bytes.len());
            md5_context.consume(bytes);
        }
        let v = md5_context.compute().to_vec();
        let md5 = v.iter().map(|x| format!("{:02x}", x)).fold(String::new(), |mut s, b| {
            s.push_str(&b);
            s
        });
        md5
    }
    serve(
        tokio::net::TcpListener::bind("[::]:9003").await.expect("fail to bind"),
        Router::new().route("/md5", axum::routing::post(md5)),
    )
    .await
    .expect("fail to serve");
}
