use clap::Parser;
use spacegate_shell::BoxError;
mod args;
fn main() -> Result<(), BoxError> {
    // TODO: more subscriber required
    tracing_subscriber::fmt().with_env_filter(tracing_subscriber::EnvFilter::from_default_env()).init();
    let args = args::Args::parse();
    #[allow(unused_variables)]
    if let Some(plugins) = args.plugins {
        #[cfg(feature = "dylib")]
        {
            let dir = std::fs::read_dir(plugins)?;
            for entry in dir {
                let entry = entry?;
                let path = entry.path();
                let ext = path.extension();
                let is_dylib = if cfg!(target_os = "windows") {
                    ext == Some("dll".as_ref())
                } else if cfg!(target_os = "macos") {
                    ext == Some("dylib".as_ref())
                } else {
                    ext == Some("so".as_ref())
                };
                if path.is_file() && is_dylib {
                    println!("loading plugin lib: {:?}", path);
                    let res = unsafe { spacegate_shell::plugin::PluginRepository::global().register_dylib(&path) };
                    if let Err(e) = res {
                        eprintln!("fail to load plugin: {:?}", e);
                    }
                }
            }
        }
        #[cfg(not(feature = "dylib"))]
        {
            eprintln!("feature dylib not enabled")
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
