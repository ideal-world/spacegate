use std::{collections::HashMap, env, time::Duration, vec};

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use spacegate_kernel::config::{
    gateway_dto::{SgGateway, SgListener},
    http_route_dto::{SgBackendRef, SgHttpRoute, SgHttpRouteRule},
};
use tardis::{
    basic::result::TardisResult,
    config::config_dto::{CacheConfig, DBConfig, FrameworkConfig, MQConfig, MailConfig, OSConfig, SearchConfig, TardisConfig, WebServerConfig},
    tokio::{self, sync::broadcast::Sender, time::sleep},
    web::{
        poem::web::{
            websocket::{BoxWebSocketUpgraded, WebSocket},
            Data,
        },
        poem_openapi::{self, param::Path, payload::Html},
        web_client::{TardisHttpResponse, TardisWebClient},
        web_server::TardisWebServer,
        ws_processor::{ws_broadcast, ws_echo, TardisWebsocketResp},
    },
    TardisFuns,
};

#[tokio::test]
async fn test_webscoket() -> TardisResult<()> {
    env::set_var("RUST_LOG", "info,spacegate_kernel=trace");
    TardisFuns::init_conf(TardisConfig {
        cs: Default::default(),
        fw: FrameworkConfig {
            app: Default::default(),
            web_server: WebServerConfig {
                enabled: true,
                port: 8080,
                ..Default::default()
            },
            web_client: Default::default(),
            cache: CacheConfig {
                enabled: false,
                ..Default::default()
            },
            db: DBConfig {
                enabled: false,
                ..Default::default()
            },
            mq: MQConfig {
                enabled: false,
                ..Default::default()
            },
            search: SearchConfig {
                enabled: false,
                ..Default::default()
            },
            mail: MailConfig {
                enabled: false,
                ..Default::default()
            },
            os: OSConfig {
                enabled: false,
                ..Default::default()
            },
            ..Default::default()
        },
    })
    .await?;
    tokio::spawn(async { TardisFuns::web_server().add_route_with_ws(WsApi, 100).await.start().await });
    spacegate_kernel::do_startup(
        SgGateway {
            name: "test_gw".to_string(),
            listeners: vec![SgListener { port: 8888, ..Default::default() }],
            ..Default::default()
        },
        vec![SgHttpRoute {
            gateway_name: "test_gw".to_string(),
            rules: Some(vec![SgHttpRouteRule {
                backends: Some(vec![SgBackendRef {
                    name_or_host: "127.0.0.1".to_string(),
                    port: 8080,
                    ..Default::default()
                }]),
                ..Default::default()
            }]),
            ..Default::default()
        }],
    )
    .await?;
    sleep(Duration::from_millis(4000000)).await;
    // let client = TardisWebClient::init(100)?;
    // let resp: TardisHttpResponse<Value> = client
    //     .post(
    //         "https://localhost:8888/post?dd",
    //         &json!({
    //             "name":"星航",
    //             "age":6
    //         }),
    //         None,
    //     )
    //     .await?;
    // assert!(resp.body.unwrap().get("data").unwrap().to_string().contains("星航"));
    Ok(())
}

pub struct WsApi;

#[poem_openapi::OpenApi]
impl WsApi {
    #[oai(path = "/echo", method = "get")]
    async fn echo(&self) -> Html<&'static str> {
        Html(
            r###"
    <body>
        <form id="loginForm">
            Name: <input id="nameInput" type="text" />
            <button type="submit">Login</button>
        </form>
        
        <form id="sendForm" hidden>
            Text: <input id="msgInput" type="text" />
            <button type="submit">Send</button>
        </form>
        
        <textarea id="msgsArea" cols="50" rows="30" hidden></textarea>
    </body>
    <script>
        let ws;
        const loginForm = document.querySelector("#loginForm");
        const sendForm = document.querySelector("#sendForm");
        const nameInput = document.querySelector("#nameInput");
        const msgInput = document.querySelector("#msgInput");
        const msgsArea = document.querySelector("#msgsArea");
        
        nameInput.focus();
        loginForm.addEventListener("submit", function(event) {
            event.preventDefault();
            loginForm.hidden = true;
            sendForm.hidden = false;
            msgsArea.hidden = false;
            msgInput.focus();
            ws = new WebSocket("ws://" + location.host + "/ws/echo/" + nameInput.value);
            ws.onmessage = function(event) {
                msgsArea.value += event.data + "\r\n";
            }
        });
        
        sendForm.addEventListener("submit", function(event) {
            event.preventDefault();
            ws.send(msgInput.value);
            msgInput.value = "";
        });
    </script>
    "###,
        )
    }

    #[oai(path = "/broadcast", method = "get")]
    async fn broadcast(&self) -> Html<&'static str> {
        Html(
            r###"
    <body>
        <form id="loginForm">
            Name: <input id="nameInput" type="text" />
            <button type="submit">Login</button>
        </form>
        
        <form id="sendForm" hidden>
            Text: <input id="msgInput" type="text" /> Receiver name: <input id="recNameInput" type="text" />
            <button type="submit">Send</button>
        </form>
        
        <textarea id="msgsArea" cols="50" rows="30" hidden></textarea>
    </body>
    <script>
        let ws;
        const loginForm = document.querySelector("#loginForm");
        const sendForm = document.querySelector("#sendForm");
        const nameInput = document.querySelector("#nameInput");
        const msgInput = document.querySelector("#msgInput");
        const recNameInput = document.querySelector("#recNameInput");
        const msgsArea = document.querySelector("#msgsArea");
        
        nameInput.focus();
        loginForm.addEventListener("submit", function(event) {
            event.preventDefault();
            loginForm.hidden = true;
            sendForm.hidden = false;
            msgsArea.hidden = false;
            msgInput.focus();
            ws = new WebSocket("ws://" + location.host + "/ws/broadcast/" + nameInput.value);
            ws.onmessage = function(event) {
                msgsArea.value += event.data + "\r\n";
            }
        });
        
        sendForm.addEventListener("submit", function(event) {
            event.preventDefault();
            ws.send(JSON.stringify({"from_avatar": nameInput.value, "msg": {"to": recNameInput.value, "msg": msgInput.value}}));
            recNameInput.value = "";
            msgInput.value = "";
        });
    </script>
    "###,
        )
    }

    #[oai(path = "/ws/echo/:name", method = "get")]
    async fn ws_echo(&self, name: Path<String>, websocket: WebSocket) -> BoxWebSocketUpgraded {
        ws_echo(
            name.0,
            HashMap::new(),
            websocket,
            |req_session, msg, _| async move {
                let resp = format!("echo:{msg} by {req_session}");
                Some(resp)
            },
            |_, _| async move {},
        )
    }

    #[oai(path = "/ws/broadcast/:name", method = "get")]
    async fn ws_broadcast(&self, name: Path<String>, websocket: WebSocket, sender: Data<&Sender<String>>) -> BoxWebSocketUpgraded {
        ws_broadcast(
            vec![name.0],
            false,
            false,
            HashMap::from([("some_key".to_string(), "ext_value".to_string())]),
            websocket,
            sender.clone(),
            |req_msg, ext| async move {
                let example_msg = TardisFuns::json.json_to_obj::<WebsocketExample>(req_msg.msg).unwrap();
                Some(TardisWebsocketResp {
                    msg: TardisFuns::json.obj_to_json(&TardisResult::Ok(format!("echo:{}, ext info:{}", example_msg.msg, ext.get("some_key").unwrap()))).unwrap(),
                    to_avatars: if example_msg.to.is_empty() { vec![] } else { vec![example_msg.to] },
                    ignore_avatars: vec![],
                })
            },
            |_, _| async move {},
        )
    }
}

#[derive(Deserialize, Serialize)]
pub struct WebsocketExample {
    pub msg: String,
    pub to: String,
}
