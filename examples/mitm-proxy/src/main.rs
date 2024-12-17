use proxy::MitmProxy;
use spacegate_kernel::{backend_service::get_http_backend_service, listener::SgListen, BoxError, CancellationToken};
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

mod clap;
mod proxy;
mod resolver;

#[tokio::main]
async fn main() -> Result<(), BoxError> {
    let args = clap::args();
    // create a gateway service
    rustls::crypto::ring::default_provider().install_default().expect("install default provider failed");
    tracing_subscriber::registry().with(tracing_subscriber::EnvFilter::from_default_env()).with(tracing_subscriber::fmt::layer()).init();
    tracing::debug!(?args);
    let cancel = CancellationToken::default();
    // or replace it with your own service
    let mitm_service = get_http_backend_service();
    let proxy_service = MitmProxy::new(mitm_service).as_service();
    let listener = SgListen::new(args.addr(), cancel.child_token()).with_service(proxy_service.clone().http());
    // start listen
    let handle = listener.spawn();
    // wait for ctrl-c
    tokio::signal::ctrl_c().await?;
    cancel.cancel();
    handle.await??;
    Ok(())
}
