use http::{Request, Response};
use hyper::Body;
use serde::{Deserialize, Serialize};
use std::sync::Arc;

use tardis::{basic::result::TardisResult, tokio::sync::Mutex};

#[cfg(feature = "cache")]
use crate::functions::{self, cache_client};
#[cfg(not(feature = "cache"))]
use lazy_static::lazy_static;
#[cfg(not(feature = "cache"))]
use std::collections::HashMap;
#[cfg(not(feature = "cache"))]
use tardis::tokio::sync::RwLock;
#[cfg(feature = "cache")]
use tardis::{cache::cache_client::TardisCacheClient, TardisFuns};
#[cfg(not(feature = "cache"))]
lazy_static! {
    static ref SERVER_STATUS: Arc<RwLock<HashMap<String, Status>>> = <_>::default();
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

pub(crate) async fn create_status_html(
    _: Request<Body>,
    _gateway_name: Arc<Mutex<String>>,
    _cache_key: Arc<Mutex<String>>,
    title: Arc<Mutex<String>>,
) -> Result<Response<Body>, hyper::Error> {
    let keys;
    #[cfg(feature = "cache")]
    {
        let cache_client = functions::cache_client::get(&_gateway_name.lock().await).await.expect("get cache client error!");
        let cache_key = _cache_key.lock().await;
        keys = cache_client.hkeys(&cache_key).await.expect("get cache keys error!");
    }
    #[cfg(not(feature = "cache"))]
    {
        let status = SERVER_STATUS.read().await;
        keys = status.keys().cloned().collect::<Vec<String>>();
    }
    let mut service_html = "".to_string();
    for key in keys {
        let status;
        #[cfg(feature = "cache")]
        {
            let cache_client = functions::cache_client::get(&_gateway_name.lock().await).await.expect("get cache client error!");
            let cache_key = _cache_key.lock().await;
            status = get_status(&key, &cache_key, &cache_client).await.expect("");
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
    let title = &title.lock().await;
    let html = STATUS_TEMPLATE.replace("{title}", title).replace("{status}", &service_html);

    Ok(Response::new(Body::from(html)))
}

#[cfg(feature = "cache")]
pub(crate) async fn update_status(server_name: &str, _cache_key: &str, client: impl AsRef<TardisCacheClient>, status: Status) -> TardisResult<()> {
    client.as_ref().hset(_cache_key, server_name, &TardisFuns::json.obj_to_string(&status)?).await?;
    Ok(())
}
#[cfg(not(feature = "cache"))]
pub(crate) async fn update_status(server_name: &str, status: Status) -> TardisResult<()> {
    let mut server_status = SERVER_STATUS.write().await;
    server_status.insert(server_name.to_string(), status);
    Ok(())
}

#[cfg(feature = "cache")]
pub(crate) async fn get_status(server_name: &str, cache_key: &str, client: impl AsRef<TardisCacheClient>) -> TardisResult<Option<Status>> {
    match client.as_ref().hget(cache_key, server_name).await? {
        Some(result) => Ok(Some(TardisFuns::json.str_to_obj(&result)?)),
        None => Ok(None),
    }
}
#[cfg(not(feature = "cache"))]
pub(crate) async fn get_status(server_name: &str) -> TardisResult<Option<Status>> {
    let server_status = SERVER_STATUS.read().await;
    Ok(server_status.get(server_name).cloned())
}

#[cfg(feature = "cache")]
pub(crate) async fn clean_status(cache_key: &str, gateway_name: &str) -> TardisResult<()> {
    let client = cache_client::get(gateway_name).await?;
    client.as_ref().del(cache_key).await?;
    Ok(())
}

#[cfg(not(feature = "cache"))]
pub(crate) async fn clean_status() -> TardisResult<()> {
    let mut server_status = SERVER_STATUS.write().await;
    server_status.clear();

    Ok(())
}
