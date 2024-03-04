use spacegate_shell::BoxError;
use tardis::{basic::tracing::TardisTracing, tokio};

fn main() -> Result<(), BoxError> {
    TardisTracing::initializer().with_env_layer().with_fmt_layer().init();
    let ns_from_env = std::env::var("NAMESPACE").ok();
    let ns_from_arg = std::env::args().nth(1);
    let namespaces = ns_from_arg.or(ns_from_env);
    let rt = tokio::runtime::Builder::new_multi_thread().enable_all().thread_name("spacegate").build().expect("fail to build runtime");
    rt.block_on(async move {
        let local_set = tokio::task::LocalSet::new();
        local_set
            .run_until(async move {
                let join_handle = spacegate_shell::startup_k8s(namespaces.as_deref()).await.expect("fail to start spacegate");
                join_handle.await.expect("join handle error")
            })
            .await
    })
}
