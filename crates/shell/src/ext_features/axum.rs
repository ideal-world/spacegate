use spacegate_config::service::ListenEvent;
use spacegate_ext_axum::axum::{self, http, routing::post, Extension, Json, Router};
use tokio::sync::mpsc::Sender;
#[derive(Debug, Clone)]
pub struct App {
    pub listen_event_tx: Sender<ListenEvent>,
}
/// Axum Api Router
pub fn shell_routers(router: Router) -> Router {
    router.nest("/control", control_routes()).route("/health", axum::routing::get(axum::Json(true))).fallback(axum::routing::any(axum::response::Html(
        axum::body::Bytes::from_static(include_bytes!("./axum/static_resource/web-server-index.html")),
    )))
}

pub fn control_routes() -> Router {
    Router::new().route("/push_event", post(event)).route("/observability", axum::routing::get(observability))
}

/// 仅返回本进程的脱敏生效值，管理端通过已有实例发现地址读取。
async fn observability() -> Json<spacegate_config::model::GatewayObservabilityRuntime> {
    let mut gateways = crate::server::RunningSgGateway::global_store()
        .lock()
        .expect("gateway store lock")
        .values()
        .map(|gateway| gateway.gateway_name.to_string())
        .collect::<Vec<_>>();
    gateways.sort();
    Json(spacegate_config::model::GatewayObservabilityRuntime {
        gateways,
        observability: crate::observability::runtime_snapshot(),
    })
}

pub struct HttpEventListener {}

pub async fn event(state: Extension<App>, event: Json<ListenEvent>) -> Result<(), http::StatusCode> {
    let send_result = state.listen_event_tx.send(event.0).await;
    if let Err(_e) = send_result {
        Err(http::StatusCode::INTERNAL_SERVER_ERROR)
    } else {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 页面只能得到进程已初始化的快照，不能读取保存值或当前环境来冒充生效值。
    #[tokio::test]
    async fn runtime_observability_is_frozen_and_redacts_endpoint_credentials() {
        let mut config = spacegate_config::model::ObservabilityConfig::default();
        config.service_name = "runtime-service".into();
        config.otlp_endpoint = "https://user:private@collector:4317/otlp?token=private#private".into();
        crate::observability::init(&config);
        // 再次初始化不得把页面显示值换成尚未生效的新配置。
        config.service_name = "not-applied".into();
        crate::observability::init(&config);
        let mut item = spacegate_config::model::ConfigItem::default();
        item.gateway.name = "runtime-served".into();
        let gateway = crate::server::RunningSgGateway::create(item, tokio_util::sync::CancellationToken::new()).unwrap();
        crate::server::RunningSgGateway::global_save("saved-config-key", gateway);
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let url = format!("http://{}/control/observability", listener.local_addr().unwrap());
        let task = tokio::spawn(async move {
            axum::serve(listener, shell_routers(Router::new())).await.unwrap();
        });
        let response = reqwest::get(url).await.unwrap();
        assert!(
            response.headers().get("content-type").unwrap().to_str().unwrap().starts_with("application/json"),
            "runtime endpoint must return a snapshot, not the legacy HTML fallback"
        );
        let body = response.text().await.unwrap();
        assert!(body.contains("runtime-service"));
        assert!(body.contains("https://collector:4317/otlp"));
        assert!(!body.contains("private") && !body.contains("not-applied"));
        assert!(body.contains("\"traces_active\":false"));
        assert!(body.contains("runtime-served") && !body.contains("saved-config-key"), "report runtime names, not file configuration keys");
        crate::server::RunningSgGateway::global_remove("saved-config-key").unwrap().shutdown().await;
        task.abort();
    }
}
