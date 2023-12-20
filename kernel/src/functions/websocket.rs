use std::net::SocketAddr;

use http::header::{CONNECTION, SEC_WEBSOCKET_ACCEPT, SEC_WEBSOCKET_KEY, SEC_WEBSOCKET_VERSION, UPGRADE};
use hyper::header::HeaderValue;
use hyper::{self};
use hyper::{Body, Request, Response, StatusCode};
use kernel_common::inner_model::gateway::SgProtocol;
use std::sync::Arc;
use tardis::basic::{error::TardisError, result::TardisResult};
use tardis::crypto::crypto_digest::algorithm;
use tardis::futures::stream::StreamExt;
use tardis::futures_util::SinkExt;
use tardis::web::tokio_tungstenite::tungstenite::{protocol, Message};
use tardis::web::tokio_tungstenite::{connect_async, WebSocketStream};
use tardis::{log, tokio, TardisFuns};

use crate::instance::SgBackendInst;

pub async fn process(gateway_name: Arc<String>, remote_addr: SocketAddr, backend: &SgBackendInst, mut request: Request<Body>) -> TardisResult<Response<Body>> {
    let have_upgrade = request
        .headers()
        .get(CONNECTION)
        .map(|v| {
            let if_have_upgrade =
                v.to_str().map_err(|e| TardisError::bad_request(&format!("[SG.Websocket] header {CONNECTION} value is illegal: {e}"), ""))?.to_lowercase().contains("upgrade");
            Ok::<_, TardisError>(!if_have_upgrade)
        })
        .transpose()?
        .unwrap_or(false);
    if have_upgrade {
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
        key.to_str().map_err(|e| TardisError::bad_request(&format!("[SG.Websocket] header {SEC_WEBSOCKET_KEY} value is illegal: {e}"), ""))?.to_string()
    } else {
        return Err(TardisError::bad_request(
            &format!("[SG.Websocket] Websocket key missing , from {remote_addr} @ {gateway_name}"),
            "",
        ));
    };

    let default_protocol = if backend.port == 443 { SgProtocol::Wss } else { SgProtocol::Ws };
    let scheme = backend.protocol.as_ref().unwrap_or(&default_protocol);
    let client_url = format!(
        "{}://{}{}{}",
        scheme,
        format_args!("{}{}", backend.name_or_host, backend.namespace.as_ref().map(|n| format!(".{n}")).unwrap_or("".to_string())),
        if (backend.port == 0 || backend.port == 80) && (scheme == &SgProtocol::Http || scheme == &SgProtocol::Ws)
            || (backend.port == 0 || backend.port == 443) && (scheme == &SgProtocol::Https || scheme == &SgProtocol::Wss)
        {
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
                let ws_service_stream = WebSocketStream::from_raw_socket(upgraded, protocol::Role::Server, None).await;
                let (mut service_write, mut service_read) = ws_service_stream.split();

                let gateway_name_clone = gateway_name.clone();

                tokio::task::spawn(async move {
                    loop {
                        match service_read.next().await {
                            Some(Ok(message)) => {
                                if let Message::Close(frame) = &message {
                                    let code = frame.as_ref().map(|f| f.code.to_string()).unwrap_or_default();
                                    log::trace!("[SG.Websocket] Gateway receive close message: (code:{code} {message}) from {remote_addr} @ {gateway_name_clone}",);
                                } else {
                                    log::trace!("[SG.Websocket] Gateway receive and forward message: {message} from {remote_addr} @ {gateway_name_clone}");
                                }
                                if let Err(error) = client_write.send(message).await {
                                    log::warn!("[SG.Websocket] Forward message error: {error} from {remote_addr} @ {gateway_name_clone}");
                                    return;
                                }
                            }
                            Some(Err(error)) => {
                                log::warn!("[SG.Websocket] Gateway receive message error: {error} from {remote_addr} @ {gateway_name_clone}");
                                return;
                            }
                            None => {
                                return;
                            }
                        }
                    }
                });

                let gateway_name = gateway_name.clone();
                tokio::task::spawn(async move {
                    loop {
                        match client_read.next().await {
                            Some(Ok(message)) => {
                                if let Message::Close(frame) = &message {
                                    let code = frame.as_ref().map(|f| f.code.to_string()).unwrap_or_default();
                                    log::trace!("[SG.Websocket] Client receive close message: (code:{code} {message}) from {remote_addr} @ {gateway_name}",);
                                } else {
                                    log::trace!("[SG.Websocket] Client receive and reply message: {message} from {remote_addr} @ {gateway_name}");
                                }
                                log::trace!("[SG.Websocket] Client receive and reply message: {message} from {remote_addr} @ {gateway_name}");
                                if let Err(error) = service_write.send(message).await {
                                    log::warn!("[SG.Websocket] Reply message error: {error} from {remote_addr} @ {gateway_name}");
                                    return;
                                }
                            }
                            Some(Err(error)) => {
                                log::warn!("[SG.Websocket] Client receive message error: {error} from {remote_addr} @ {gateway_name}");
                                return;
                            }
                            None => {
                                return;
                            }
                        }
                    }
                });
            }
            Err(error) => {
                log::warn!("[SG.Websocket] Upgrade error: {error} from {remote_addr} @ {gateway_name}");
            }
        }
    });
    let accept_key = TardisFuns::crypto.base64.encode_raw(TardisFuns::crypto.digest.digest_bytes::<algorithm::Sha1>(format!("{request_key}258EAFA5-E914-47DA-95CA-C5AB0DC85B11"))?);

    let mut response = Response::new(Body::empty());
    *response.status_mut() = StatusCode::SWITCHING_PROTOCOLS;

    response.headers_mut().insert(UPGRADE, HeaderValue::from_static("websocket"));
    response.headers_mut().insert(CONNECTION, HeaderValue::from_static("Upgrade"));
    response.headers_mut().insert(SEC_WEBSOCKET_ACCEPT, accept_key.parse().map_err(|_| TardisError::bad_request("", ""))?);
    Ok(response)
}
