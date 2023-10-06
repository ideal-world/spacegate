use tardis::{basic::result::TardisResult, tokio, TardisFuns};

mod api;
mod config;
mod constants;
mod dto;
mod initializer;
mod service;

#[tokio::main]
async fn main() -> TardisResult<()> {
    TardisFuns::init(Some("config")).await?;
    let web_server = TardisFuns::web_server();
    initializer::init(web_server).await?;
    web_server.start().await?;
    web_server.await;
    Ok(())
}
