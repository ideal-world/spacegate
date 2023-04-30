use spacegate_kernel::config::{gateway_dto::SgGateway, http_route_dto::SgHttpRoute};
use tardis::{
    basic::{error::TardisError, result::TardisResult},
    tokio,
};

#[tokio::main]
async fn main() -> TardisResult<()> {
    let gateway_config_path = std::env::args().nth(1).expect("The first parameter is missing: gateway configuration path");
    let routes_config_path = std::env::args().nth(2).expect("The second parameter is missing: routes configuration path");
    let gateway_config_content = tokio::fs::read_to_string(&gateway_config_path).await?;
    if gateway_config_content.is_empty() {
        return Err(TardisError::not_found(&format!("[SG.Config] Gateway Config not found in {gateway_config_path}"), ""));
    }
    let gateway_config = tardis::TardisFuns::json.str_to_obj::<SgGateway>(&gateway_config_content)?;

    let mut routes_config_dir = tokio::fs::read_dir(&routes_config_path).await?;
    let mut routes_config_content = Vec::new();
    while let Some(route_config_dir) = routes_config_dir.next_entry().await? {
        routes_config_content.push(tokio::fs::read_to_string(&route_config_dir.path()).await?);
    }
    if routes_config_content.is_empty() {
        return Err(TardisError::not_found(&format!("[SG.Config] Routes Config not found in {routes_config_path}"), ""));
    }
    let routes_configs = routes_config_content.into_iter().map(|v| tardis::TardisFuns::json.str_to_obj::<SgHttpRoute>(&v).unwrap()).collect();

    spacegate_kernel::do_startup(gateway_config, routes_configs).await?;
    Ok(())
}
