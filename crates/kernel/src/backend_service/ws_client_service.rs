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
    Ok(())
}
