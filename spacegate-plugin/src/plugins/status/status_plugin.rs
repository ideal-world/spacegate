#![allow(unused_assignments)]
use http_body_util::Full;
use hyper::{Request, Response};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tower::BoxError;

type BoxResult<T> = Result<T, BoxError>;
#[cfg(not(feature = "cache"))]
use std::collections::HashMap;
#[cfg(not(feature = "cache"))]
use tardis::tardis_static;
#[cfg(not(feature = "cache"))]
use tardis::tokio::sync::RwLock;
#[cfg(feature = "cache")]
use tardis::{cache::cache_client::TardisCacheClient, TardisFuns};
#[cfg(not(feature = "cache"))]
tardis_static! {
    server_status: Arc<RwLock<HashMap<String, Status>>> = <_>::default();
}
const STATUS_TEMPLATE: &str = include_str!("status.html");

#[derive(Default, Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) enum Status {
    #[default]
    Good,
    Minor,
    Major,
}

impl Status {
    fn to_html_css_class(&self) -> String {
        match self {
            Status::Good => "good".to_string(),
            Status::Minor => "minor".to_string(),
            Status::Major => "major".to_string(),
        }
    }
}

pub(crate) async fn create_status_html<B>(
    _: Request<B>,
    _gateway_name: Arc<str>,
    _cache_key: Arc<str>,
    title: Arc<str>,
) -> Result<Response<Full<hyper::body::Bytes>>, hyper::Error> {
    let mut keys = Vec::<String>::new();
    #[cfg(feature = "cache")]
    {
        let cache_client = crate::cache::Cache::get(_gateway_name.as_ref()).await.expect("get cache client error!");
        keys = cache_client.hkeys(_cache_key.as_ref()).await.expect("get cache keys error!");
    }
    #[cfg(not(feature = "cache"))]
    {
        let status = server_status().read().await;
        keys = status.keys().cloned().collect::<Vec<String>>();
    }
    let mut service_html = "".to_string();
    for ref key in keys {
        let status;
        #[cfg(feature = "cache")]
        {
            let cache_client = crate::cache::Cache::get(_gateway_name.as_ref()).await.expect("get cache client error!");
            status = get_status(key, _cache_key.as_ref(), &cache_client).await.expect("");
        }
        #[cfg(not(feature = "cache"))]
        {
            status = get_status(&key).await.expect("");
        }
        if let Some(status) = status {
            service_html.push_str(
                format!(
                    r##"<div class="service">
                            <div class="service-name">{}</div>
                            <div class="service-status {}">Status</div>
                        </div>"##,
                    key,
                    status.to_html_css_class()
                )
                .as_str(),
            );
        };
    }
    let html = STATUS_TEMPLATE.replace("{title}", title.as_ref()).replace("{status}", &service_html);

    Ok(Response::new(Full::new(html.into())))
}

#[cfg(feature = "cache")]
pub(crate) async fn update_status(server_name: &str, _cache_key: &str, client: impl AsRef<TardisCacheClient>, status: Status) -> BoxResult<()> {
    client.as_ref().hset(_cache_key, server_name, &TardisFuns::json.obj_to_string(&status)?).await?;
    Ok(())
}
#[cfg(not(feature = "cache"))]
pub(crate) async fn update_status(server_name: &str, status: Status) -> BoxResult<()> {
    let mut server_status = server_status().write().await;
    server_status.insert(server_name.to_string(), status);
    Ok(())
}

#[cfg(feature = "cache")]
pub(crate) async fn get_status(server_name: &str, cache_key: &str, client: impl AsRef<TardisCacheClient>) -> BoxResult<Option<Status>> {
    match client.as_ref().hget(cache_key, server_name).await? {
        Some(result) => Ok(Some(TardisFuns::json.str_to_obj(&result)?)),
        None => Ok(None),
    }
}
#[cfg(not(feature = "cache"))]
pub(crate) async fn get_status(server_name: &str) -> BoxResult<Option<Status>> {
    let server_status = server_status().read().await;
    Ok(server_status.get(server_name).cloned())
}

#[cfg(feature = "cache")]
pub(crate) async fn clean_status(cache_key: &str, gateway_name: &str) -> BoxResult<()> {
    let client = crate::cache::Cache::get(gateway_name).await?;
    client.as_ref().del(cache_key).await?;
    Ok(())
}

#[cfg(not(feature = "cache"))]
pub(crate) async fn clean_status(cache_key: &str, gateway_name: &str) -> BoxResult<()> {
    let mut server_status = server_status().write().await;
    server_status.clear();

    Ok(())
}
