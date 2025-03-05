use std::{
    collections::HashMap,
    sync::{Arc, OnceLock},
    time::Duration,
};

use axum::{
    extract::{Query, State},
    routing::get,
    Json, Router,
};
use serde::Deserialize;
use serde_json::Value;
use spacegate_config::{
    service::{ConfigEventType, ConfigType, Discovery, Instance, ListenEvent},
    BackendHost, BoxError, PluginAttributes,
};
use tokio::{sync::RwLock, time::Instant};

use crate::{error::InternalError, state::AppState};

const DEFAULT_TIMEOUT: Duration = Duration::from_secs(5);
const DEFAULT_HEALTH_CACHE_EXPIRE: Duration = Duration::from_secs(1);
static HEALTH_CACHE: OnceLock<Arc<RwLock<(bool, Instant)>>> = OnceLock::new();

fn health_cache() -> Arc<RwLock<(bool, Instant)>> {
    HEALTH_CACHE.get_or_init(|| Arc::new(RwLock::new((false, Instant::now())))).clone()
}

async fn set_health_cache(health: bool) {
    let expire = Instant::now() + DEFAULT_HEALTH_CACHE_EXPIRE;
    let cache = health_cache();
    let mut wg = cache.write().await;
    *wg = (health, expire)
}

async fn get_health_cache() -> Option<bool> {
    let cache = health_cache();
    let (health, expire) = *cache.read().await;
    if expire.elapsed() >= Duration::ZERO {
        None
    } else {
        Some(health)
    }
}

pub struct InstanceApi<'i, I: Instance> {
    pub instance: &'i I,
    timeout: Duration,
}

impl<'i, I: Instance> InstanceApi<'i, I> {
    pub fn new(instance: &'i I) -> Self {
        Self {
            instance,
            timeout: DEFAULT_TIMEOUT,
        }
    }
    /// get api url
    pub fn url(&self, path: &str) -> String {
        format!("http://{base}/{path}", base = self.instance.api_url(), path = path.trim_start_matches('/'))
    }
    /// check server health
    pub async fn health(&self) -> bool {
        if let Some(health) = get_health_cache().await {
            health
        } else {
            use reqwest::Client;
            let client = Client::default();
            let timeout = DEFAULT_TIMEOUT;
            let health = client.get(self.url("/health")).timeout(timeout).send().await.is_ok_and(|x| x.status().is_success());
            set_health_cache(health).await;
            health
        }
    }

    pub async fn schema(&self, plugin_code: &str) -> Result<Option<Value>, BoxError> {
        let resp = reqwest::Client::new()
            .get(format!("http://{base}/plugin-schema?code={plugin_code}", base = self.instance.api_url()))
            .timeout(self.timeout)
            .send()
            .await?
            .json::<Option<Value>>()
            .await?;
        Ok(resp)
    }

    pub async fn plugin_list(&self) -> Result<Vec<PluginAttributes>, BoxError> {
        let attrs = reqwest::Client::new()
            .get(format!("http://{base}/plugin-list", base = self.instance.api_url()))
            .timeout(self.timeout)
            .send()
            .await?
            .json::<Vec<PluginAttributes>>()
            .await?;
        Ok(attrs)
    }

    pub async fn push_event(&self, event: &ListenEvent) -> Result<(), BoxError> {
        let resp = reqwest::Client::new().post(format!("http://{base}/control/push_event", base = self.instance.api_url())).timeout(self.timeout).json(event).send().await?;
        if resp.status().is_success() {
            Ok(())
        } else {
            let status = resp.status();
            let message = resp.text().await?;
            let resp = format!("fail to transfer request, status: {status}, message: {message}");
            Err(resp.into())
        }
    }
}

async fn instance_health<B: Discovery>(State(AppState { backend, .. }): State<AppState<B>>) -> Result<Json<HashMap<String, bool>>, InternalError> {
    let api = backend.instances().await.map_err(InternalError)?;
    let mut healths = HashMap::new();
    for instance in api {
        healths.insert(instance.id().to_string(), InstanceApi::new(&instance).health().await);
    }
    Ok(Json(healths))
}

async fn instance_list<B: Discovery>(State(AppState { backend, .. }): State<AppState<B>>) -> Result<Json<Vec<String>>, InternalError> {
    let api = backend.instances().await.map_err(InternalError)?.into_iter().map(|instance| instance.id().to_owned()).collect();
    Ok(Json(api))
}

async fn backends<B: Discovery>(State(AppState { backend, .. }): State<AppState<B>>) -> Result<Json<Vec<BackendHost>>, InternalError> {
    backend.backends().await.map(Json).map_err(InternalError)
}

#[derive(Debug, Deserialize)]
pub enum ReloadKind {
    Route,
    Gateway,
    Global,
}

#[derive(Debug, Deserialize)]
pub struct ReloadGatewayQuery {
    instance: String,
    gateway: String,
}

#[derive(Debug, Deserialize)]
pub struct ReloadGlobalQuery {
    instance: String,
}

#[derive(Debug, Deserialize)]
pub struct ReloadRouteQuery {
    instance: String,
    gateway: String,
    route: String,
}

async fn reload_gateway<B: Discovery>(
    State(AppState { backend, .. }): State<AppState<B>>,
    Query(ReloadGatewayQuery { gateway, instance }): Query<ReloadGatewayQuery>,
) -> Result<(), InternalError> {
    let instances = backend.instances().await.map_err(InternalError)?;
    for i in instances {
        if i.id() == instance {
            let event = ListenEvent {
                r#type: ConfigEventType::Update,
                config: ConfigType::Gateway { name: gateway },
            };
            let client = InstanceApi::new(&i);
            client.push_event(&event).await.map_err(InternalError)?;
            return Ok(());
        }
    }
    Err(InternalError("instance not found".into()))
}

async fn reload_global<B: Discovery>(
    State(AppState { backend, .. }): State<AppState<B>>,
    Query(ReloadGlobalQuery { instance }): Query<ReloadGlobalQuery>,
) -> Result<(), InternalError> {
    let instances = backend.instances().await.map_err(InternalError)?;
    for i in instances {
        if i.id() == instance {
            let event = ListenEvent {
                r#type: ConfigEventType::Update,
                config: ConfigType::Global,
            };
            let client = InstanceApi::new(&i);
            client.push_event(&event).await.map_err(InternalError)?;
            return Ok(());
        }
    }
    Err(InternalError("instance not found".into()))
}

async fn reload_route<B: Discovery>(
    State(AppState { backend, .. }): State<AppState<B>>,
    Query(ReloadRouteQuery { instance, route, gateway }): Query<ReloadRouteQuery>,
) -> Result<(), InternalError> {
    let instances = backend.instances().await.map_err(InternalError)?;
    for i in instances {
        if i.id() == instance {
            let event = ListenEvent {
                r#type: ConfigEventType::Update,
                config: ConfigType::Route {
                    gateway_name: gateway,
                    name: route,
                },
            };
            let client = InstanceApi::new(&i);
            client.push_event(&event).await.map_err(InternalError)?;
            return Ok(());
        }
    }
    Err(InternalError("instance not found".into()))
}

pub fn router<B>() -> axum::Router<AppState<B>>
where
    B: Discovery + Send + Sync + 'static,
{
    Router::new()
        .nest(
            "/instance",
            Router::new().route("/health", get(instance_health::<B>)).route("/list", get(instance_list::<B>)).nest(
                "/reload",
                Router::new().route("/gateway", get(reload_gateway::<B>)).route("/global", get(reload_global::<B>)).route("/route", get(reload_route::<B>)),
            ),
        )
        .route("/backends", get(backends::<B>))
}
