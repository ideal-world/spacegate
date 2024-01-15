use std::{
    collections::HashMap,
    env,
    sync::{
        atomic::{AtomicUsize, Ordering},
        Arc,
    },
    time::Duration,
    vec,
};

use lazy_static::lazy_static;
use serde_json::json;
use spacegate_kernel::config::http_route_dto::{SgHttpPathMatch, SgHttpPathMatchType, SgHttpRouteMatch};
use spacegate_kernel::config::plugin_filter_dto::SgRouteFilter;
use spacegate_kernel::config::{
    gateway_dto::{SgGateway, SgListener},
    http_route_dto::{SgBackendRef, SgHttpRoute, SgHttpRouteRule},
    plugin_filter_dto,
};
use spacegate_kernel::plugins::filters;
use tardis::config::config_dto::WebServerCommonConfig;
use tardis::web::web_server::WebServerModule;
use tardis::web::ws_processor::TardisWebsocketMgrMessage;
use tardis::{
    basic::result::TardisResult,
    config::config_dto::{FrameworkConfig, TardisConfig, WebServerConfig},
    tokio::{
        self,
        sync::{broadcast::Sender, RwLock},
        time::sleep,
    },
    web::{
        poem::web::websocket::{BoxWebSocketUpgraded, WebSocket},
        poem_openapi::{self, param::Path},
        tokio_tungstenite::tungstenite::Message,
        ws_processor::{ws_broadcast, ws_echo, TardisWebsocketReq, TardisWebsocketResp},
    },
    TardisFuns,
};

lazy_static! {
    static ref SENDERS: Arc<RwLock<HashMap<String, Sender<TardisWebsocketMgrMessage>>>> = Arc::new(RwLock::new(HashMap::new()));
}

#[tokio::test]
async fn test_webscoket() -> TardisResult<()> {
    env::set_var("RUST_LOG", "info,spacegate_kernel=trace,tardis=trace");
    tracing_subscriber::fmt::init();

    TardisFuns::init_conf(TardisConfig {
        cs: Default::default(),
        fw: FrameworkConfig {
            app: Default::default(),
            web_server: Some(WebServerConfig::builder().common(WebServerCommonConfig::builder().port(8081).build()).default(Default::default()).build()),
            ..Default::default()
        },
    })
    .await?;
    tokio::spawn(async { TardisFuns::web_server().add_route(WebServerModule::from(WsApi).with_ws::<String>(100)).await.start().await });
    spacegate_kernel::do_startup(
        SgGateway {
            name: "test_gw".to_string(),
            listeners: vec![SgListener { port: 8080, ..Default::default() }],
            ..Default::default()
        },
        vec![SgHttpRoute {
            gateway_name: "test_gw".to_string(),
            rules: Some(vec![SgHttpRouteRule {
                backends: Some(vec![SgBackendRef {
                    name_or_host: "127.0.0.1".to_string(),
                    port: 8081,
                    ..Default::default()
                }]),
                matches: Some(vec![SgHttpRouteMatch {
                    path: Some(SgHttpPathMatch {
                        kind: SgHttpPathMatchType::Prefix,
                        value: "/".to_string(),
                    }),
                    ..Default::default()
                }]),
                filters: Some(vec![SgRouteFilter {
                    code: "rewrite".to_string(),
                    name: None,
                    spec: TardisFuns::json.obj_to_json(&filters::rewrite::SgFilterRewrite {
                        hostname: None,
                        path: Some(plugin_filter_dto::SgHttpPathModifier {
                            kind: plugin_filter_dto::SgHttpPathModifierType::ReplacePrefixMatch,
                            value: "/".to_string(),
                        }),
                    })?,
                }]),
                ..Default::default()
            }]),
            ..Default::default()
        }],
    )
    .await?;
    sleep(Duration::from_millis(500)).await;

    static ERROR_COUNTER: AtomicUsize = AtomicUsize::new(0);
    static SUB_COUNTER: AtomicUsize = AtomicUsize::new(0);
    static NON_SUB_COUNTER: AtomicUsize = AtomicUsize::new(0);

    // close message
    let close_client_a = TardisFuns::ws_client("ws://127.0.0.1:8080/ws/broadcast/gerror/a", move |_| async move { None }).await?;
    close_client_a.send_text("hi".parse()?).await?;
    close_client_a.send_raw(Message::Close(None)).await.unwrap();

    // message not illegal test
    let error_client_a = TardisFuns::ws_client("ws://127.0.0.1:8080/ws/broadcast/gerror/a", move |msg| async move {
        if let Message::Text(msg) = msg {
            println!("client_not_found recv:{}", msg);
            assert_eq!(msg, r#"{"msg":"message illegal","event":"__sys_error__"}"#);
            ERROR_COUNTER.fetch_add(1, Ordering::SeqCst);
        }
        None
    })
    .await?;
    error_client_a.send_text("hi".to_string()).await?;
    // not found test
    let error_client_b = TardisFuns::ws_client("ws://127.0.0.1:8080/ws/broadcast/gxxx/a", move |msg| async move {
        if let Message::Text(msg) = msg {
            println!("client_not_found recv:{}", msg);
            assert_eq!(msg, "Websocket connection error: group not found");
            ERROR_COUNTER.fetch_add(1, Ordering::SeqCst);
        }
        None
    })
    .await?;
    error_client_b
        .send_obj(&TardisWebsocketReq {
            msg: json! {"hi"},
            from_avatar: "a".to_string(),
            ..Default::default()
        })
        .await?;

    // subscribe mode test
    let sub_client_a = TardisFuns::ws_client("ws://127.0.0.1:8080/ws/broadcast/g1/a", move |msg| async move {
        if let Message::Text(msg) = msg {
            println!("client_a recv:{}", msg);
            assert_eq!(msg, r#"{"msg":"service send:\"hi\"","event":null}"#);
            SUB_COUNTER.fetch_add(1, Ordering::SeqCst);
        }
        None
    })
    .await?;
    let sub_client_b1 = TardisFuns::ws_client("ws://127.0.0.1:8080/ws/broadcast/g1/b", move |msg| async move {
        if let Message::Text(msg) = msg {
            println!("client_b1 recv:{}", msg);
            assert_eq!(msg, r#"{"msg":"service send:\"hi\"","event":null}"#);
            SUB_COUNTER.fetch_add(1, Ordering::SeqCst);
            Some(Message::Text(
                TardisFuns::json
                    .obj_to_string(&TardisWebsocketReq {
                        msg: json! {"client_b send:hi again"},
                        from_avatar: "b".to_string(),
                        ..Default::default()
                    })
                    .unwrap(),
            ))
        } else {
            None
        }
    })
    .await?;
    let sub_client_b2 = TardisFuns::ws_client("ws://127.0.0.1:8080/ws/broadcast/g1/b", move |msg| async move {
        if let Message::Text(msg) = msg {
            println!("client_b2 recv:{}", msg);
            assert_eq!(msg, r#"{"msg":"service send:\"hi\"","event":null}"#);
            SUB_COUNTER.fetch_add(1, Ordering::SeqCst);
            Some(Message::Text(
                TardisFuns::json
                    .obj_to_string(&TardisWebsocketReq {
                        msg: json! {"client_b send:hi again"},
                        from_avatar: "b".to_string(),
                        ..Default::default()
                    })
                    .unwrap(),
            ))
        } else {
            None
        }
    })
    .await?;
    sub_client_a
        .send_obj(&TardisWebsocketReq {
            msg: json! {"hi"},
            from_avatar: "a".to_string(),
            ..Default::default()
        })
        .await?;
    sub_client_b1
        .send_obj(&TardisWebsocketReq {
            msg: json! {"hi"},
            from_avatar: "b".to_string(),
            ..Default::default()
        })
        .await?;
    sub_client_b2
        .send_obj(&TardisWebsocketReq {
            msg: json! {"hi"},
            from_avatar: "b".to_string(),
            ..Default::default()
        })
        .await?;

    // non-subscribe mode test
    let non_sub_client_a = TardisFuns::ws_client("ws://127.0.0.1:8080/ws/broadcast/g2/a", move |msg| async move {
        if let Message::Text(msg) = msg {
            println!("client_a recv:{}", msg);
            assert_eq!(msg, r#"{"msg":"service send:\"hi\"","event":null}"#);
            NON_SUB_COUNTER.fetch_add(1, Ordering::SeqCst);
        }
        None
    })
    .await?;
    let non_sub_client_b1 = TardisFuns::ws_client("ws://127.0.0.1:8080/ws/broadcast/g2/b", move |msg| async move {
        if let Message::Text(msg) = msg {
            println!("client_b1 recv:{}", msg);
            assert_eq!(msg, r#"{"msg":"service send:\"hi\"","event":null}"#);
            NON_SUB_COUNTER.fetch_add(1, Ordering::SeqCst);
            Some(Message::Text(
                TardisFuns::json
                    .obj_to_string(&TardisWebsocketReq {
                        msg: json! {"client_b send:hi again"},
                        from_avatar: "b".to_string(),
                        ..Default::default()
                    })
                    .unwrap(),
            ))
        } else {
            None
        }
    })
    .await?;
    let non_sub_client_b2 = TardisFuns::ws_client("ws://127.0.0.1:8080/ws/broadcast/g2/b", move |msg| async move {
        if let Message::Text(msg) = msg {
            println!("client_b2 recv:{}", msg);
            assert_eq!(msg, r#"{"msg":"service send:\"hi\"","event":null}"#);
            NON_SUB_COUNTER.fetch_add(1, Ordering::SeqCst);
            Some(Message::Text(
                TardisFuns::json
                    .obj_to_string(&TardisWebsocketReq {
                        msg: json! {"client_b send:hi again"},
                        from_avatar: "b".to_string(),
                        ..Default::default()
                    })
                    .unwrap(),
            ))
        } else {
            None
        }
    })
    .await?;

    non_sub_client_a
        .send_obj(&TardisWebsocketReq {
            msg: json! {"hi"},
            from_avatar: "a".to_string(),
            ..Default::default()
        })
        .await?;
    non_sub_client_b1
        .send_obj(&TardisWebsocketReq {
            msg: json! {"hi"},
            from_avatar: "b".to_string(),
            ..Default::default()
        })
        .await?;
    non_sub_client_b2
        .send_obj(&TardisWebsocketReq {
            msg: json! {"hi"},
            from_avatar: "b".to_string(),
            ..Default::default()
        })
        .await?;

    sleep(Duration::from_millis(500)).await;
    assert_eq!(ERROR_COUNTER.load(Ordering::SeqCst), 2);
    assert_eq!(SUB_COUNTER.load(Ordering::SeqCst), 6);
    assert_eq!(NON_SUB_COUNTER.load(Ordering::SeqCst), 5);

    Ok(())
}

#[derive(Clone)]
pub struct WsApi;

#[poem_openapi::OpenApi]
impl WsApi {
    #[oai(path = "/ws/broadcast/:group/:name", method = "get")]
    async fn ws_broadcast(&self, group: Path<String>, name: Path<String>, websocket: WebSocket) -> BoxWebSocketUpgraded {
        if !SENDERS.read().await.contains_key(&group.0) {
            SENDERS.write().await.insert(group.0.clone(), tokio::sync::broadcast::channel::<TardisWebsocketMgrMessage>(100).0);
        }
        let sender = SENDERS.read().await.get(&group.0).unwrap().clone();
        if group.0 == "g1" {
            ws_broadcast(
                vec![name.0],
                false,
                true,
                HashMap::new(),
                websocket,
                sender,
                |req_msg, _ext| async move {
                    println!("service g1 recv:{}:{}", req_msg.from_avatar, req_msg.msg);
                    if req_msg.msg == json! {"client_b send:hi again"} {
                        return None;
                    }
                    Some(TardisWebsocketResp {
                        msg: json! { format!("service send:{}", TardisFuns::json.json_to_string(req_msg.msg).unwrap())},
                        to_avatars: vec![],
                        ignore_avatars: vec![],
                    })
                },
                |_, _| async move {},
            )
            .await
        } else if group.0 == "g2" {
            ws_broadcast(
                vec![name.0],
                false,
                false,
                HashMap::new(),
                websocket,
                sender,
                |req_msg, _ext| async move {
                    println!("service g2 recv:{}:{}", req_msg.from_avatar, req_msg.msg);
                    if req_msg.msg == json! {"client_b send:hi again"} {
                        return None;
                    }
                    Some(TardisWebsocketResp {
                        msg: json! { format!("service send:{}", TardisFuns::json.json_to_string(req_msg.msg).unwrap())},
                        to_avatars: vec![],
                        ignore_avatars: vec![],
                    })
                },
                |_, _| async move {},
            )
            .await
        } else if group.0 == "gerror" {
            ws_broadcast(
                vec![name.0],
                false,
                false,
                HashMap::new(),
                websocket,
                sender,
                |req_msg, _ext| async move {
                    println!("service gerror recv:{}:{}", req_msg.from_avatar, req_msg.msg);
                    None
                },
                |_, _| async move {},
            )
            .await
        } else {
            ws_echo(
                name.0,
                HashMap::new(),
                websocket,
                |_, _, _| async move { Some("Websocket connection error: group not found".to_string()) },
                |_, _| async move {},
            )
        }
    }
}
