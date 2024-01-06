use futures_util::{SinkExt, StreamExt};
use hyper::upgrade::Upgraded;
use hyper_util::rt::TokioIo;
use tokio::{self};

use tower::BoxError;

use tokio_tungstenite::{tungstenite::protocol::Role, WebSocketStream};
pub async fn service(as_server: Upgraded, as_client: Upgraded) -> Result<(), BoxError> {
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
                    tracing::trace!(role = "client", "[SG.Websocket] Gateway recieve message {message}");
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
    });
    Ok(())
}
