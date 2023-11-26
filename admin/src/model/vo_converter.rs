use tardis::async_trait::async_trait;
use tardis::basic::result::TardisResult;

pub mod gateway_vo_conv;
pub mod http_route_conv;
pub mod plugin_vo_conv;

#[async_trait]
pub trait VoConv<M, S>
where
    S: VoConv<M, S>,
{
    async fn to_model(self, client_name: &str) -> TardisResult<M>;
    async fn from_model(model: M) -> TardisResult<S>;
}
