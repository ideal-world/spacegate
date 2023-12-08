use tardis::async_trait::async_trait;
use tardis::basic::result::TardisResult;
use tardis::futures_util::future::join_all;

pub mod gateway_vo_conv;
pub mod http_route_conv;
pub mod plugin_vo_conv;

#[async_trait]
pub trait VoConv<M, S>
where
    M: Send,
    S: VoConv<M, S> + Send,
{
    async fn to_model(self, client_name: &str) -> TardisResult<M>;
    async fn from_model(model: M) -> TardisResult<S>;

    async fn to_vec_model(client_name: &str, vo: Vec<S>) -> TardisResult<Vec<M>>
    where
        S: 'async_trait,
    {
        join_all(vo.into_iter().map(|s| s.to_model(client_name)).collect::<Vec<_>>()).await.into_iter().collect::<TardisResult<Vec<_>>>()
    }

    async fn from_vec_model(models: Option<Vec<M>>) -> TardisResult<Vec<S>>
    where
        M: 'async_trait,
    {
        let result = if let Some(models) = models {
            join_all(models.into_iter().map(S::from_model).collect::<Vec<_>>()).await.into_iter().collect::<TardisResult<Vec<_>>>()?
        } else {
            vec![]
        };
        Ok(result)
    }
}
