use std::{collections::HashMap, sync::Arc};

use lazy_static::lazy_static;
use tardis::{
    tokio::{self, sync::Mutex, task},
    web::poem::{handler, web::Html},
};
lazy_static! {
    static ref SERVER_STATUS: Arc<Mutex<HashMap<String, Status>>> = <_>::default();
}
const STATUS_TEMPLATE: &str = include_str!("status.html");

pub(crate) enum Status {
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
pub(crate) async fn create_status_html() -> Html<String> {
    let status = SERVER_STATUS.lock().await;
    let mut html = "".to_string();
    status.keys().for_each(|key| {
        let status = status.get(key).expect("");
        html.push_str(
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
    Html(STATUS_TEMPLATE.replace("{status}", &html))
}
pub(crate) async fn update_status(server_name: &str, status: Status) {
    let mut server_status = SERVER_STATUS.lock().await;
    server_status.insert(server_name.to_string(), status);
}
pub(crate) async fn remove_status(server_name: &str) {
    let mut server_status = SERVER_STATUS.lock().await;
    server_status.remove(server_name);
}
