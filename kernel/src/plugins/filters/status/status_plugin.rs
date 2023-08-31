use http::{Request, Response};
use hyper::Body;
use serde::{Deserialize, Serialize};
use std::sync::Arc;

use tardis::{basic::result::TardisResult, cache::cache_client::TardisCacheClient, tokio::sync::Mutex, TardisFuns};

use crate::functions;

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
    gateway_name: Arc<Mutex<String>>,
    cache_key: Arc<Mutex<String>>,
    title: Arc<Mutex<String>>,
) -> Result<Response<Body>, hyper::Error> {
    let cache_client = functions::cache_client::get(&gateway_name.lock().await).await.expect("get cache client error!");
    let cache_key = cache_key.lock().await;
    let keys = cache_client.hkeys(&cache_key).await.expect("get cache keys error!");
    let mut service_html = "".to_string();
    for key in keys {
        if let Some(status) = get_status(&key, &cache_key, &cache_client).await.expect("") {
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

pub(crate) async fn update_status(server_name: &str, cache_key: &str, client: impl AsRef<TardisCacheClient>, status: Status) -> TardisResult<()> {
    client.as_ref().hset(cache_key, server_name, &TardisFuns::json.obj_to_string(&status)?).await?;
    Ok(())
}

pub(crate) async fn get_status(server_name: &str, cache_key: &str, client: impl AsRef<TardisCacheClient>) -> TardisResult<Option<Status>> {
    match client.as_ref().hget(cache_key, server_name).await? {
        Some(result) => Ok(Some(TardisFuns::json.str_to_obj(&result)?)),
        None => Ok(None),
    }
}

pub(crate) async fn clean_status(cache_key: &str, client: &TardisCacheClient) -> TardisResult<()> {
    client.del(cache_key).await?;
    Ok(())
}
