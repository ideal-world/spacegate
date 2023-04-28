use std::collections::HashMap;
use std::net::SocketAddr;

use crate::config::gateway_dto::SgProtocol;
use crate::config::http_route_dto::SgBackendRef;
use http::header::{CONNECTION, SEC_WEBSOCKET_ACCEPT, SEC_WEBSOCKET_KEY, SEC_WEBSOCKET_VERSION, UPGRADE};
use hyper::header::HeaderValue;
use hyper::{self};
use hyper::{Body, Request, Response, StatusCode};
use std::sync::Arc;
use tardis::basic::{error::TardisError, result::TardisResult};
use tardis::futures::stream::StreamExt;
use tardis::futures_util::{future, SinkExt, TryStreamExt};
use tardis::web::poem::error;
use tardis::web::tokio_tungstenite::tungstenite::{protocol, Message};
use tardis::web::tokio_tungstenite::{connect_async, WebSocketStream};
use tardis::web::ws_client::TardisWSClient;
use tardis::{log, tokio, TardisFuns};

pub async fn process(gateway_name: Arc<String>, remote_addr: SocketAddr, backend: &SgBackendRef, mut request: Request<Body>) -> TardisResult<Response<Body>> {
    if request.headers().get(CONNECTION).map(|v| !v.to_str().unwrap().to_lowercase().contains("upgrade")).unwrap_or(false) {
        return Err(TardisError::bad_request(
            &format!("[SG.Websocket] Connection header must be upgrade , from {remote_addr} @ {gateway_name}"),
            "",
        ));
    }
    if let Some(version) = request.headers().get(SEC_WEBSOCKET_VERSION) {
        if version != "13" {
            return Err(TardisError::bad_request(
                &format!("[SG.Websocket] Websocket protocol version must be 13 , from {remote_addr} @ {gateway_name}"),
                "",
            ));
        }
    }
    let request_key = if let Some(key) = request.headers().get(SEC_WEBSOCKET_KEY) {
        key.to_str().unwrap().to_string()
    } else {
        return Err(TardisError::bad_request(
            &format!("[SG.Websocket] Websocket key missing , from {remote_addr} @ {gateway_name}"),
            "",
        ));
    };

    let scheme = backend.protocol.as_ref().unwrap_or(&SgProtocol::Ws);
    let client_url = format!(
        "{}://{}{}{}",
        scheme,
        format!("{}{}", backend.namespace.as_ref().map(|n| format!("{n}.")).unwrap_or("".to_string()), backend.name_or_host),
        if (backend.port == 0 || backend.port == 80) && scheme == &SgProtocol::Http || (backend.port == 0 || backend.port == 443) && scheme == &SgProtocol::Https {
            "".to_string()
        } else {
            format!(":{}", backend.port)
        },
        request.uri().path_and_query().map(|p| p.as_str()).unwrap_or("")
    );

    tokio::task::spawn(async move {
        log::trace!("[SG.Websocket] Connection client url: {client_url} , from {remote_addr} @ {gateway_name}");
        let ws_client_stream = match connect_async(client_url.clone()).await {
            Ok((ws_client_stream, _)) => ws_client_stream,
            Err(error) => {
                log::warn!("[SG.Websocket] Connection client url: {client_url} error: {error} from {remote_addr} @ {gateway_name}");
                return;
            }
        };
        let (mut client_write, mut client_read) = ws_client_stream.split();
        match hyper::upgrade::on(&mut request).await {
            Ok(upgraded) => {
                let mut ws_service_stream = WebSocketStream::from_raw_socket(upgraded, protocol::Role::Server, None).await;
                while let Some(Ok(message)) = ws_service_stream.next().await {
                    if let Err(error) = client_write.send(message).await {
                        log::warn!("[SG.Websocket] Client send error: {error} from {remote_addr} @ {gateway_name}");
                        return;
                    }
                    match client_read.next().await {
                        Some(Ok(message)) => {
                            if let Err(error) = ws_service_stream.send(message).await {
                                log::warn!("[SG.Websocket] Reply error: {error} from {remote_addr} @ {gateway_name}");
                                return;
                            }
                        }
                        Some(Err(error)) => {
                            log::warn!("[SG.Websocket] Client receive error: {error} from {remote_addr} @ {gateway_name}");
                            return;
                        }
                        _ => {}
                    }
                }
            }
            Err(error) => {
                log::warn!("[SG.Websocket] Upgrade error: {error} from {remote_addr} @ {gateway_name}");
                return;
            }
        }
    });
    let accept_key = TardisFuns::crypto.base64.encode_raw(&TardisFuns::crypto.digest.digest_raw(
        &format!("{request_key}258EAFA5-E914-47DA-95CA-C5AB0DC85B11").as_bytes(),
        tardis::crypto::rust_crypto::sha1::Sha1::new(),
    )?);

    let mut response = Response::new(Body::empty());
    *response.status_mut() = StatusCode::SWITCHING_PROTOCOLS;

    response.headers_mut().insert(UPGRADE, HeaderValue::from_static("websocket"));
    response.headers_mut().insert(CONNECTION, HeaderValue::from_static("Upgrade"));
    response.headers_mut().insert(SEC_WEBSOCKET_ACCEPT, accept_key.parse().unwrap());
    Ok(response)
}
