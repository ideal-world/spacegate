use std::{net::IpAddr, str::FromStr, sync::Arc, time::Duration};

use super::{status_plugin, SgFilterStatusConfig};
use hyper::{body::Incoming, service::service_fn, Request};
use hyper_util::rt::{TokioExecutor, TokioIo};
use tardis::tokio::{self};
use tokio_util::sync::CancellationToken;
use tower::BoxError;
use tracing::instrument;
#[instrument(skip(cancel_signal, config))]
pub async fn launch_status_server(config: &SgFilterStatusConfig, gateway_name: Arc<str>, cancel_signal: CancellationToken) -> Result<(), BoxError> {
    let host = IpAddr::from_str(&config.host)?;
    let port = config.port;
    // just wait 500ms for prev server to shutdown
    let bind_fail_instant = tokio::time::Instant::now() + Duration::from_millis(500);
    let listener = loop {
        match tokio::net::TcpListener::bind((host, port)).await {
            Ok(listener) => break listener,
            Err(e) => {
                if std::io::ErrorKind::AddrInUse == e.kind() && bind_fail_instant.elapsed().is_zero() {
                    tokio::task::yield_now().await;
                    continue;
                } else {
                    tracing::warn!("[Sg.Plugin.Status] fail to bind {host}:{port}, error: {e}");
                    return Err(Box::new(e));
                }
            }
        }
    };
    let cache_key = Arc::<str>::from(config.status_cache_key.clone().as_str());
    let title = Arc::<str>::from(config.title.clone().as_str());
    'accept_loop: loop {
        let (stream, _peer) = tokio::select! {
            _ = cancel_signal.cancelled() => {
                tracing::info!("[Sg.Plugin.Status] cancelled by cancel signal");
                break 'accept_loop;
            }
            _ = tokio::signal::ctrl_c() => {
                tracing::info!("[Sg.Plugin.Status] cancelled by ctrl+c signal");
                break 'accept_loop;
            }
            accept = listener.accept() => {
                match accept {
                    Ok(incoming) => incoming,
                    Err(e) => {
                        tracing::error!("[Sg.Plugin.Status] Status server accept error: {:?}", e);
                        continue 'accept_loop;
                    }
                }
            }
        };
        let gateway_name = gateway_name.clone();
        let cache_key = cache_key.clone();
        let title = title.clone();
        tokio::spawn(async move {
            let connector = hyper_util::server::conn::auto::Builder::new(TokioExecutor::default());
            let connection = connector.serve_connection(
                TokioIo::new(stream),
                service_fn(move |req: Request<Incoming>| Box::pin(status_plugin::create_status_html(req, gateway_name.clone(), cache_key.clone(), title.clone()))),
            );
            let result = connection.await;
            if let Err(e) = result {
                tracing::error!("[Sg.Plugin.Status] Status server connection error: {:?}", e);
            }
        });
    }
    status_plugin::clean_status(&cache_key, &gateway_name).await?;
    Ok(())
}
