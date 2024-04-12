use clap::Parser;
use spacegate_shell::BoxError;
mod args;
fn main() -> Result<(), BoxError> {
    tracing_subscriber::fmt().with_env_filter(tracing_subscriber::EnvFilter::from_default_env()).init();
    let args = args::Args::parse();
    let rt = tokio::runtime::Builder::new_multi_thread().enable_all().thread_name(env!("CARGO_PKG_NAME")).build().expect("fail to build runtime");
    rt.block_on(async move {
        match args.config {
            #[cfg(feature = "fs")]
            args::Config::File(path) => spacegate_shell::startup_file(path).await,
            #[cfg(feature = "k8s")]
            args::Config::K8s(ns) => spacegate_shell::startup_k8s(Some(ns.as_ref())).await,
            #[cfg(feature = "redis")]
            args::Config::Redis(url) => spacegate_shell::startup_redis(url).await,
        }
    })
}
