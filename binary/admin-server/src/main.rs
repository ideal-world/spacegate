use axum::Router;
use spacegate_config::service::*;
use state::AppState;
use std::{net::SocketAddr, sync::Arc};
pub mod clap;
pub mod mw;
pub mod service;

pub mod error;
pub mod state;
pub trait Backend: Create + Retrieve + Update + Delete + Send + Sync + 'static {}

impl<T> Backend for T where T: Create + Retrieve + Update + Delete + Send + Sync + 'static {}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    use tracing_subscriber::{fmt, prelude::*, EnvFilter};
    tracing_subscriber::registry().with(fmt::layer()).with(EnvFilter::from_default_env()).init();
    let args = <crate::clap::Args as ::clap::Parser>::parse();
    tracing::info!("server started with args: {:?}", args);
    let addr = SocketAddr::new(args.host, args.port);
    let sec = args.key.map(|k| k.0.into());
    let digest: Option<Arc<[u8; 32]>> = args.sk.map(|sk| {
        let out = <sha2::Sha256 as digest::Digest>::digest(sk);
        let out: [u8; 32] = out.into();
        Arc::new(out)
    });
    // let schemas = args.schemas.load_all()?;
    let app = match args.config {
        clap::ConfigBackend::File(path) => {
            let backend = spacegate_config::service::fs::Fs::new(path, config_format::Json::default());
            create_app(backend, sec, digest)
        }
        clap::ConfigBackend::K8s(ns) => {
            let backend = spacegate_config::service::k8s::K8s::with_default_client(ns).await?;
            create_app(backend, sec, digest)
        }
    };
    let listener = tokio::net::TcpListener::bind(addr).await?;
    axum::serve(listener, app)
        .with_graceful_shutdown(async move {
            let _ = tokio::signal::ctrl_c().await;
        })
        .await?;
    Ok(())
}

/// create app for an backend
pub fn create_app<B>(backend: B, sec: Option<Arc<[u8]>>, sk_digest: Option<Arc<[u8; 32]>>) -> Router<()>
where
    B: Discovery + Create + Retrieve + Update + Delete + Send + Sync + 'static,
{
    let state = AppState {
        backend: Arc::new(backend),
        version: mw::version_control::Version::new(),
        secret: sec,
        sk_digest,
        // plugin_schemas: Arc::new(schemas.into()),
    };
    service::router(state)
}
