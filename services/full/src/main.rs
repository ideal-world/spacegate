use tardis::{basic::result::TardisResult, tokio, TardisFuns};

#[tokio::main]
async fn main() -> TardisResult<()> {
    TardisFuns::init_log()?;
    let namespaces = std::env::args().nth(1).map(Some).unwrap_or(None);
    spacegate_kernel::startup_k8s(namespaces).await.expect("fail to start up");
    spacegate_kernel::wait_graceful_shutdown().await
}
