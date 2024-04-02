use spacegate_shell::BoxError;
use tardis::{basic::tracing::TardisTracing, tokio};

#[tokio::main]
async fn main() -> Result<(), BoxError> {
    TardisTracing::initializer().with_env_layer().with_fmt_layer().init();
    let conf_url = std::env::args().nth(1).expect("The first parameter is missing: configuration connection url");
    let check_interval_sec = std::env::args().nth(2).expect("The second parameter is missing: configuration change check period (in seconds)").parse()?;
    let rt = tokio::runtime::Builder::new_multi_thread().enable_all().thread_name("spacegate").build().expect("fail to build runtime");
    rt.block_on(async move {
        let local_set = tokio::task::LocalSet::new();
        local_set
            .run_until(async move {
                let join_handle = spacegate_shell::startup_cache(&conf_url, check_interval_sec).await.expect("fail to start spacegate");
                join_handle.await.expect("join handle error")
            })
            .await
    })
}
