use std::{net::SocketAddr, sync::Arc};

use futures_util::TryFutureExt;
use tokio_util::sync::CancellationToken;
use tracing::{instrument, Instrument};

use crate::{service::TcpService, BoxError, BoxResult};

/// Listener embodies the concept of a logical endpoint where a Gateway accepts network connections.
#[derive(Clone)]
pub struct SgListen {
    pub socket_addr: SocketAddr,
    pub services: Vec<Arc<dyn TcpService>>,
    pub listener_id: String,
    cancel_token: CancellationToken,
}

impl std::fmt::Debug for SgListen {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SgListen")
            .field("socket_addr", &self.socket_addr)
            .field("listener_id", &self.listener_id)
            .field("services", &self.services.iter().map(|s| s.protocol_name()).collect::<Vec<_>>())
            .finish_non_exhaustive()
    }
}

impl SgListen {
    /// we only have 65535 ports for a console, so it's a safe size
    pub fn new(socket_addr: SocketAddr, cancel_token: CancellationToken) -> Self {
        Self {
            socket_addr,
            services: Vec::new(),
            cancel_token,
            listener_id: Default::default(),
        }
    }

    pub fn with_service<S: TcpService>(mut self, service: S) -> Self {
        self.services.push(Arc::new(service));
        self
    }

    pub fn add_service<S: TcpService>(&mut self, service: S) {
        self.services.push(Arc::new(service));
    }

    pub fn with_services(mut self, services: Vec<Arc<dyn TcpService>>) -> Self {
        self.services.extend(services);
        self
    }

    pub fn extend_services(&mut self, services: Vec<Arc<dyn TcpService>>) {
        self.services.extend(services);
    }

    pub fn with_listener_id(mut self, listener_id: impl Into<String>) -> Self {
        self.listener_id = listener_id.into();
        self
    }
}

impl SgListen {
    /// Spawn the listener on the tokio runtime.
    ///
    /// It's a shortcut for `tokio::spawn(listener.listen())`.
    pub fn spawn(self) -> tokio::task::JoinHandle<Result<(), BoxError>> {
        tokio::spawn(self.listen())
    }

    /// Listen on the socket address.
    #[instrument(skip(self), fields(bind=%self.socket_addr))]
    pub async fn listen(self) -> Result<(), BoxError> {
        tracing::debug!("start binding...");
        let listener = tokio::net::TcpListener::bind(self.socket_addr).await?;
        let cancel_token = self.cancel_token;
        tracing::debug!("start listening...");
        let peek_size = self.services.iter().fold(0, |acc, s| acc.max(s.sniff_peek_size()));
        let services: Arc<[Arc<dyn TcpService>]> = self.services.clone().into();
        loop {
            let accepted = tokio::select! {
                () = cancel_token.cancelled() => {
                    tracing::warn!("cancelled");
                    return Ok(());
                },
                accepted = listener.accept() => accepted
            };
            match accepted {
                Ok((stream, peer_addr)) => {
                    let services = services.clone();
                    let _task = tokio::spawn(
                        async move {
                            let mut peek_buf = vec![0u8; peek_size];
                            stream.peek(&mut peek_buf).await?;
                            for s in services.iter() {
                                if s.sniff(&peek_buf) {
                                    tracing::debug!(tcp_service=%s.protocol_name(), "accepted");
                                    s.handle(stream, peer_addr).await?;
                                    break;
                                }
                            }
                            BoxResult::Ok(())
                        }
                        .inspect_err(|e| {
                            tracing::warn!("TcpService error: {:?}", e);
                        })
                        .instrument(tracing::info_span!("connection", peer = %peer_addr)),
                    );
                }
                Err(e) => {
                    tracing::warn!("Accept tcp connection error: {:?}", e);
                }
            }
        }
    }
}
