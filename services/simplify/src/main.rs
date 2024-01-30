use spacegate_kernel::BoxError;
use tardis::{basic::tracing::TardisTracing, tokio};

fn main() -> Result<(), BoxError> {
    TardisTracing::initializer().with_env_layer().with_fmt_layer().init();
    let conf_path = std::env::args().nth(1).expect("The first parameter is missing: configuration path");
    let rt = tokio::runtime::Builder::new_multi_thread().enable_all().thread_name("spacegate").build().expect("fail to build runtime");
    rt.block_on(async move {
        let local_set = tokio::task::LocalSet::new();
        local_set
            .run_until(async move {
                let join_handle = spacegate_kernel::startup_file(conf_path).await.expect("fail to start spacegate");
                join_handle.await.expect("join handle error")
            })
            .await
    })
}
