use std::{net::SocketAddr, str::FromStr};

use spacegate_kernel::{
    layers::{
        gateway,
        http_route::{SgHttpBackendLayer, SgHttpRoute, SgHttpRouteRuleLayer},
    },
    listener::SgListen,
    service::{get_http_backend_service, http_backend_service},
};
use tokio_util::sync::CancellationToken;
use tower_layer::Layer;

#[tokio::test]
async fn test_multi_part() {
    tokio::spawn(gateway());
    tokio::spawn(axum_server());
    let client = reqwest::Client::new();
    client.post("[::]:9002/md5").body(reqwest::multipart::Form::new())
}

async fn gateway() {
    let cancel = CancellationToken::default();
    let gateway = gateway::SgGatewayLayer::builder("test_multi_part", cancel.clone())
        .http_routers([
            (
                "test_upload".to_string(),
                SgHttpRoute::builder()
                    .name("test_upload")
                    .rule(
                        SgHttpRouteRuleLayer::builder()
                            .match_all()
                            .backend(SgHttpBackendLayer::builder().host("[::]").port(9003).build().expect("fail to build backend"))
                            .build()
                            .expect("fail to build rule"),
                    )
                    .build()
                    .expect("fail_to_build"),
            ),
            // ("test_download".to_string(), SgHttpRoute::builder().name("test_download").build().expect("fail_to_build")),
        ])
        .build();
    let addr = SocketAddr::from_str("[::]:9002").expect("invalid host");
    let listener = SgListen::new(addr, gateway.layer(get_http_backend_service()), cancel, "listener");
    listener.listen().await.expect("fail to listen");
}

async fn axum_server() {
    use axum::{extract::Multipart, response::IntoResponse, serve, Router};
    pub async fn md5(mut multipart: Multipart) -> impl IntoResponse {
        let mut md5_context = md5::Context::new();
        while let Some(field) = multipart.next_field().await.unwrap() {
            let bytes = field.bytes().await.expect("fail to load bytes");
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
