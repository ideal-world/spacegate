use hyper::upgrade::Upgraded;
use hyper_util::rt::TokioIo;
use tokio::{self};

use crate::BoxError;

/// transfer data between 2 tcp upgraded services
pub(super) async fn tcp_transfer(as_server: Upgraded, as_client: Upgraded) -> Result<(), BoxError> {
    let mut server_conn = TokioIo::new(as_server);
    let mut client_conn = TokioIo::new(as_client);
    tokio::task::spawn(async move {
        let result = tokio::io::copy_bidirectional(&mut server_conn, &mut client_conn).await;
        match result {
            Ok((server_to_client, client_to_server)) => {
                tracing::debug!("[SG.Upgraded] connection closed, server to client bytes: {server_to_client}, client to server bytes: {client_to_server}");
            }
            Err(error) => {
                tracing::warn!("[SG.Upgraded] connection close error: {error}");
            }
        }
    });

    // we may need to check the websocket inner messages, but now we just forward the data
    /*
    let (mut as_server_tx, mut as_server_rx) = WebSocketStream::from_raw_socket(TokioIo::new(as_server), Role::Server, None).await.split();
    let (mut as_client_tx, mut as_client_rx) = WebSocketStream::from_raw_socket(TokioIo::new(as_client), Role::Client, None).await.split();
    tokio::task::spawn(async move {
        while let Some(message) = as_server_rx.next().await {
            match message {
                Ok(message) => {
                    tracing::trace!(role = "server", "[SG.Websocket] Gateway recieve message {message}");
                    match as_client_tx.send(message).await {
                        Ok(_) => {}
                        Err(error) => {
                            tracing::warn!(role = "server", "[SG.Websocket] Client send message error: {error}");
                            return;
                        }
                    }
                }
                Err(error) => {
                    tracing::warn!(role = "server", "[SG.Websocket] Gateway receive message error: {error}");
                    return;
                }
            }
        }
    });
    tokio::task::spawn(async move {
        while let Some(message) = as_client_rx.next().await {
            match message {
                Ok(message) => {
                    tracing::trace!(role = "client", "[SG.Websocket] Gateway receive message {message}");
                    match as_server_tx.send(message).await {
                        Ok(_) => {}
                        Err(error) => {
                            tracing::warn!(role = "client", "[SG.Websocket] Gateway send message error: {error}");
                            return;
                        }
                    }
                }
                Err(error) => {
                    tracing::warn!(role = "client", "[SG.Websocket] Client receive message error: {error}");
                    return;
                }
            }
        }
    }); */
    Ok(())
}
