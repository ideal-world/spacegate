use tardis::basic::result::TardisResult;

mod config;
mod plugin;
mod server;

pub async fn init() -> TardisResult<()> {
    Ok(())
}
