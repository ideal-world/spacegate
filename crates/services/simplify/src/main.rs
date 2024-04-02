use spacegate_shell::BoxError;

fn main() -> Result<(), BoxError> {
    tracing_subscriber::fmt().with_env_filter(tracing_subscriber::EnvFilter::from_default_env()).init();
    let conf_path = std::env::args().nth(1).expect("The first parameter is missing: configuration path");
    let rt = tokio::runtime::Builder::new_multi_thread().enable_all().thread_name("spacegate").build().expect("fail to build runtime");
    rt.block_on(async move {
        let local_set = tokio::task::LocalSet::new();
        local_set
            .run_until(async move {
                let join_handle = spacegate_shell::startup_file(conf_path).await.expect("fail to start spacegate");
                join_handle.await.expect("join handle error")
            })
            .await
    })
}
