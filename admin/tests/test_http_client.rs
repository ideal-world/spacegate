use serde::{Deserialize, Serialize};
use tardis::{
    basic::{error::TardisError, result::TardisResult},
    config::config_dto::WebClientModuleConfig,
    log,
    web::{
        poem_openapi::types::{ParseFromJSON, ToJSON},
        web_client::{TardisHttpResponse, TardisWebClient},
        web_resp::{TardisResp, Void},
    },
    TardisFuns,
};

pub struct TestHttpClient {
    client: TardisWebClient,
    base_url: String,
}

impl TestHttpClient {
    pub async fn new(base_url: &str) -> TardisResult<Self> {
        let client = TardisWebClient::init(&WebClientModuleConfig {
            connect_timeout_sec: 100,
            ..Default::default()
        })?;
        Ok(TestHttpClient {
            client,
            base_url: base_url.to_string(),
        })
    }

    pub async fn get<T>(&self, path: &str, headers: impl IntoIterator<Item = (String, String)>) -> TardisResult<T>
    where
        T: for<'de> Deserialize<'de> + ParseFromJSON + ToJSON + Serialize + Send + Sync,
    {
        let resp = self.client.get::<TardisResp<T>>(format!("{}{}", self.base_url, path), headers).await?;
        Ok(resp.body.unwrap().data.unwrap())
    }

    pub async fn post<T>(&self, path: &str, body: &impl serde::Serialize, headers: impl IntoIterator<Item = (String, String)>) -> TardisResult<T>
    where
        T: for<'de> Deserialize<'de> + ParseFromJSON + ToJSON + Serialize + Send + Sync,
    {
        let resp: TardisHttpResponse<String> = self.client.post_obj_to_str(format!("{}{}", self.base_url, path), body, headers).await?;
        log::info!("[TestHttpClient] post resp body: {:?}", resp.body.as_ref().unwrap());
        Ok(TardisFuns::json.str_to_obj::<TardisResp<T>>(&resp.body.unwrap())?.data.unwrap())
    }

    pub async fn put<T>(&self, path: &str, body: &impl serde::Serialize, headers: impl IntoIterator<Item = (String, String)>) -> TardisResult<T>
    where
        T: for<'de> Deserialize<'de> + ParseFromJSON + ToJSON + Serialize + Send + Sync,
    {
        let resp: TardisHttpResponse<String> = self.client.put_obj_to_str(format!("{}{}", self.base_url, path), body, headers).await?;
        log::info!("[TestHttpClient] put resp body: {:?}", resp.body.as_ref().unwrap());
        Ok(TardisFuns::json.str_to_obj::<TardisResp<T>>(&resp.body.unwrap())?.data.unwrap())
    }

    pub async fn delete(&self, path: &str, headers: impl IntoIterator<Item = (String, String)>) -> TardisResult<()> {
        let resp: TardisHttpResponse<TardisResp<Void>> = self.client.delete(format!("{}{}", self.base_url, path), headers).await?;
        let resp = resp.body.unwrap();
        log::info!("[TestHttpClient] delete resp code: {}", resp.code);
        if resp.code != "200" {
            return Err(TardisError::internal_error(&resp.msg, ""));
        }
        Ok(())
    }
}
