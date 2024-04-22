use clap::Parser;
use spacegate_shell::BoxError;
mod args;
fn main() -> Result<(), BoxError> {
    tracing_subscriber::fmt().with_env_filter(tracing_subscriber::EnvFilter::from_default_env()).init();
    let args = args::Args::parse();
    if let Some(plugins) = args.plugins {
        let dir = std::fs::read_dir(plugins)?;
        let repo = spacegate_shell::plugin::SgPluginRepository::global();
        for entry in dir {
            let entry = entry?;
            let path = entry.path();
            if path.is_file() {
                println!("load plugin: {:?}", path);
                println!("{:?}", repo as *const spacegate_shell::plugin::SgPluginRepository);
                let res = unsafe { spacegate_shell::plugin::SgPluginRepository::global().register_lib(&path) };
                if let Err(e) = res {
                    eprintln!("fail to load plugin: {:?}", e);
                }
            }
        }
    }
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
