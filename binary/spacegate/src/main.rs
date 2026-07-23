use clap::Parser;
use spacegate_shell::BoxError;
mod args;
fn main() -> Result<(), BoxError> {
    let args = args::Args::parse();
    #[allow(unused_variables)]
    if let Some(plugins) = args.plugins {
        #[cfg(feature = "dylib")]
        {
            for plugins in plugin_dirs(&plugins) {
                let dir = match std::fs::read_dir(plugins) {
                    Ok(dir) => dir,
                    Err(e) => {
                        eprintln!("skip plugin dir {:?}: {:?}", plugins, e);
                        continue;
                    }
                };
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
            args::Config::K8s(ns) => spacegate_shell::startup_k8s_with_gateway_class(Some(ns.as_ref()), &args.gateway_class_name).await,
            #[cfg(feature = "redis")]
            args::Config::Redis(url) => spacegate_shell::startup_redis(url).await,
            args::Config::Static(s) => {
                let config = spacegate_shell::plugin::serde_json::from_reader(std::fs::File::open(s)?)?;
                spacegate_shell::startup_static(config).await
            }
        }
    })
}

/// Splits the startup plugin directory argument into concrete directories.
fn plugin_dirs(plugins: &str) -> impl Iterator<Item = &str> {
    plugins.split(',').map(str::trim).filter(|plugins| !plugins.is_empty())
}

#[cfg(test)]
mod tests {
    use super::plugin_dirs;

    #[test]
    fn plugin_dirs_parse_comma_separated_directories() {
        let dirs = plugin_dirs(" /lib/spacegate/plugins, /var/lib/spacegate/plugins ,,").collect::<Vec<_>>();

        assert_eq!(dirs, vec!["/lib/spacegate/plugins", "/var/lib/spacegate/plugins"]);
    }
}
