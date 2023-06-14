use std::{collections::HashMap, sync::Arc};

use lazy_static::lazy_static;
use poem::web::Data;
use tardis::{
    tokio::sync::RwLock,
    web::poem::{handler, web::Html},
};
//todo redis
lazy_static! {
    static ref SERVER_STATUS: Arc<RwLock<HashMap<String, Status>>> = <_>::default();
}
const STATUS_TEMPLATE: &str = include_str!("status.html");

#[derive(Default, Debug, Clone, PartialEq, Eq)]
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

#[handler]
pub(crate) async fn create_status_html(Data(title): Data<&String>) -> Html<String> {
    let status = SERVER_STATUS.read().await;
    let mut service_html = "".to_string();
    status.keys().for_each(|key| {
        let status = status.get(key).expect("");
        service_html.push_str(
            format!(
                r##"<div class="service">
                        <div class="service-name">{}</div>
                        <div class="service-status {}">状态</div>
                    </div>"##,
                key,
                status.to_html_css_class()
            )
            .as_str(),
        );
    });
    let html = STATUS_TEMPLATE.replace("{title}", title);
    Html(html.replace("{status}", &service_html))
}

pub(crate) async fn update_status(server_name: &str, status: Status) {
    let mut server_status = SERVER_STATUS.write().await;
    server_status.insert(server_name.to_string(), status);
}

pub(crate) async fn get_status(server_name: &str) -> Option<Status> {
    let server_status = SERVER_STATUS.read().await;
    server_status.get(server_name).cloned()
}

pub(crate) async fn clean_status() {
    let mut server_status = SERVER_STATUS.write().await;
    server_status.clear();
}
