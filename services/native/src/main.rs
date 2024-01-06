use tardis::{tokio, TardisFuns};

#[tokio::main]
async fn main() {
    TardisFuns::init_log().expect("fail to init log");
    let conf_url = std::env::args().nth(1).expect("The first parameter is missing: configuration connection url");
    let check_interval_sec = std::env::args().nth(2).expect("The second parameter is missing: configuration change check period (in seconds)");
    spacegate_kernel::startup_native(conf_url, check_interval_sec.parse().unwrap()).await.expect("fail to startup");
    spacegate_kernel::wait_graceful_shutdown().await.expect("fail to shutdown")
}
