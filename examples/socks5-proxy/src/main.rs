mod socks5;
use socks5::Socks5;
use spacegate_kernel::{listener::SgListen, CancellationToken};
#[tokio::main]
async fn main() -> spacegate_kernel::BoxResult<()> {
    tracing_subscriber::fmt().with_env_filter(tracing_subscriber::EnvFilter::from_default_env().add_directive(tracing_subscriber::filter::LevelFilter::DEBUG.into())).init();
    let ct = CancellationToken::new();
    let bind = "[::]:10908".parse()?;
    let listen_ct = ct.child_token();
    let handle = SgListen::new(bind, listen_ct).with_service(Socks5::new()).spawn();
    tokio::signal::ctrl_c().await?;
    ct.cancel();
    handle.await??;
    Ok(())
}
