use tardis::basic::result::TardisResult;

mod config;
mod functions;
mod plugins;

pub async fn startup(k8s_mode: bool, ext_conf_url: Option<String>) -> TardisResult<()> {
    let config = config::init(k8s_mode, ext_conf_url).await?;
    
    Ok(())
}
