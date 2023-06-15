use std::{collections::HashMap, sync::Arc, thread};

use async_trait::async_trait;
use hyper::Body;
use itertools::Itertools;
use lazy_static::lazy_static;
use serde::{Deserialize, Serialize};
use tardis::{
    basic::result::TardisResult,
    rand::{self, distributions::WeightedIndex, prelude::Distribution, thread_rng, Rng},
    tokio::sync::Mutex,
    TardisFuns,
};

use crate::{
    config::http_route_dto::SgHttpRouteRule,
    functions::{http_client, http_route::SgHttpRouteMatchInst},
};

use super::{BoxSgPluginFilter, SgPluginFilter, SgPluginFilterDef, SgRoutePluginContext};

lazy_static! {
    static ref REQUEST_BODY: Arc<Mutex<HashMap<String, Option<Vec<u8>>>>> = <_>::default();
}

pub const CODE: &str = "retry";

pub struct SgFilterRetryDef;

impl SgPluginFilterDef for SgFilterRetryDef {
    fn inst(&self, spec: serde_json::Value) -> TardisResult<BoxSgPluginFilter> {
        let filter = TardisFuns::json.json_to_obj::<SgFilterRetry>(spec)?;
        Ok(filter.boxed())
    }
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(default)]
pub struct SgFilterRetry {
    pub retries: u16,
    pub retryable_methods: Vec<String>,
    /// Backoff strategies can vary depending on the specific implementation and requirements.
    /// see [BackOff]
    pub backoff: BackOff,
    /// milliseconds
    pub base_interval: u64,
    /// milliseconds
    pub max_interval: u64,
}

impl Default for SgFilterRetry {
    fn default() -> Self {
        Self {
            retries: 3,
            retryable_methods: vec!["*".to_string()],
            backoff: BackOff::default(),
            base_interval: 100,
            //10 seconds
            max_interval: 10000,
        }
    }
}

#[derive(Debug, Default, Serialize, Deserialize, Clone)]
pub enum BackOff {
    /// Fixed interval
    Fixed,
    /// In the exponential backoff strategy, the initial delay is relatively short,
    /// but it gradually increases as the number of retries increases.
    /// Typically, the delay time is calculated by multiplying a base value with an exponential factor.
    /// For example, the delay time might be calculated as `base_value * (2 ^ retry_count)`.
    #[default]
    Exponential,
    Random,
}

#[async_trait]
impl SgPluginFilter for SgFilterRetry {
    fn kind(&self) -> super::SgPluginFilterKind {
        super::SgPluginFilterKind::Http
    }

    fn accept_error_response(&self) -> bool {
        true
    }

    async fn init(&self, _http_route_rules: &[SgHttpRouteRule]) -> TardisResult<()> {
        Ok(())
    }

    async fn destroy(&self) -> TardisResult<()> {
        Ok(())
    }

    async fn req_filter(&self, _: &str, mut ctx: SgRoutePluginContext, _matched_match_inst: Option<&SgHttpRouteMatchInst>) -> TardisResult<(bool, SgRoutePluginContext)> {
        let mut req_body_cache = REQUEST_BODY.lock().await;
        let req_body = ctx.pop_req_body().await?;
        req_body_cache.insert(ctx.get_request_id().to_string(), req_body.clone());
        if let Some(req_body) = req_body {
            ctx.set_req_body(req_body)?;
        }
        Ok((true, ctx))
    }

    async fn resp_filter(&self, _: &str, mut ctx: SgRoutePluginContext, _: Option<&SgHttpRouteMatchInst>) -> TardisResult<(bool, SgRoutePluginContext)> {
        if ctx.is_resp_error() {
            let mut req_body_cache = REQUEST_BODY.lock().await;
            let req_body = req_body_cache.remove(&ctx.get_request_id().to_string()).flatten();
            for i in 0..self.retries {
                let retry_count = i + 1;
                let backoff_interval = match self.backoff {
                    BackOff::Fixed => self.base_interval,
                    BackOff::Exponential => self.base_interval * 2u64.pow(retry_count as u32),
                    BackOff::Random => {
                        let mut rng = rand::thread_rng();
                        rng.gen_range(self.base_interval..self.max_interval)
                    }
                };
                let time_out = ctx.get_timeout_ms();
                http_client::raw_request(
                    None,
                    ctx.get_req_method().clone(),
                    &choose_backend_url(&mut ctx),
                    req_body.clone().map(Body::from),
                    ctx.get_req_headers(),
                    time_out,
                )
                .await?;
                // Wait for the backoff interval
                thread::sleep(std::time::Duration::from_millis(backoff_interval));
            }
        }

        Ok((true, ctx))
    }
}

fn choose_backend_url(ctx: &mut SgRoutePluginContext) -> String {
    let backend_name = ctx.get_chose_backend_name();
    let available_backend = ctx.get_available_backend();
    if backend_name.is_some() {
        let backend = if available_backend.len() > 1 {
            let weights = available_backend.iter().map(|backend| backend.weight.unwrap_or(0)).collect_vec();
            let dist = WeightedIndex::new(weights).expect("Unreachable code");
            let mut rng = thread_rng();
            available_backend.get(dist.sample(&mut rng))
        } else {
            available_backend.get(0)
        };
        backend.map(|backend| backend.get_base_url()).unwrap_or_else(|| "".to_string())
    } else {
        ctx.get_req_uri().to_string()
    }
}
