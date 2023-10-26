use tardis::{basic::result::TardisResult, tokio, TardisFuns};

mod api;
mod config;
mod constants;
mod helper;
mod initializer;
mod model;
mod service;

#[tokio::main]
async fn main() -> TardisResult<()> {
    // todo 根据现有的k8s资源初始化成VO
    TardisFuns::init(Some("config")).await?;
    let web_server = TardisFuns::web_server();
    initializer::init(&web_server).await?;
    web_server.start().await?;
    web_server.await;
    Ok(())
}
