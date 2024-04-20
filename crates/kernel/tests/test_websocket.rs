use futures_util::{SinkExt, StreamExt};

use std::{net::SocketAddr, str::FromStr, time::Duration};

use spacegate_kernel::{
    listener::SgListen,
    service::{
        gateway,
        http_route::{HttpBackend, HttpRoute, HttpRouteRule},
    },
};
use tokio_util::sync::CancellationToken;
#[tokio::test]
async fn test_ws() {
    tokio::spawn(gateway());
    tokio::spawn(ws_server());
    // wait for startup
    tokio::time::sleep(Duration::from_millis(200)).await;
    let (stream, _resp) = tokio_tungstenite::connect_async("ws://[::]:9003/ws").await.expect("fail to connect");
    let (mut ws_sender, mut ws_receiver) = stream.split();
    for idx in 0..10 {
        let text_msg = idx.to_string();
        ws_sender.send(tokio_tungstenite::tungstenite::Message::Text(text_msg.clone())).await.expect("fail to send");
        let msg = ws_receiver.next().await.unwrap().unwrap();
        assert_eq!(text_msg, msg.to_text().expect("fail to get text"));
    }
    ws_sender.send(tokio_tungstenite::tungstenite::Message::Close(None)).await.expect("fail to close");
    assert!(ws_receiver.next().await.expect("fail to get close frame").expect("fail to get close frame").is_close());
}

async fn gateway() {
    let cancel = CancellationToken::default();
    let gateway = gateway::Gateway::builder("test_websocket")
        .http_routers([(
            "ws".to_string(),
            HttpRoute::builder().rule(HttpRouteRule::builder().match_all().backend(HttpBackend::builder().host("[::]").port(9002).build()).build()).build(),
        )])
        .build();
    let addr = SocketAddr::from_str("[::]:9003").expect("invalid host");
    let listener = SgListen::new(addr, gateway.as_service(), cancel, "listener");
    listener.listen().await.expect("fail to listen");
}

async fn ws_server() {
    let listener = tokio::net::TcpListener::bind("[::]:9002").await.expect("fail to bind");
    while let Ok((stream, _peer)) = listener.accept().await {
        let ws_stream = tokio_tungstenite::accept_async(stream).await.expect("fail to accept ws connection");
        tokio::spawn(async move {
            let (mut ws_sender, mut ws_receiver) = ws_stream.split();
            while let Some(Ok(msg)) = ws_receiver.next().await {
                let _result = ws_sender.send(msg).await;
            }
        });
    }
}
