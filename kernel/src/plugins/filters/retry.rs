use async_trait::async_trait;
use lazy_static::lazy_static;
use serde::{Deserialize, Serialize};
use tardis::{basic::result::TardisResult, TardisFuns};

use crate::{config::http_route_dto::SgHttpRouteRule, functions::http_route::SgHttpRouteMatchInst};

use super::{BoxSgPluginFilter, SgPluginFilter, SgPluginFilterDef, SgRouteFilterContext};

lazy_static! {
    // static ref SHUTDOWN_TX: Arc<Mutex<Option<Sender<()>>>> = <_>::default();
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

    async fn req_filter(&self, _: &str, ctx: SgRouteFilterContext, _matched_match_inst: Option<&SgHttpRouteMatchInst>) -> TardisResult<(bool, SgRouteFilterContext)> {
        Ok((true, ctx))
    }

    async fn resp_filter(&self, _: &str, ctx: SgRouteFilterContext, _: Option<&SgHttpRouteMatchInst>) -> TardisResult<(bool, SgRouteFilterContext)> {
        if let Some(_backend_name) = ctx.get_chose_backend_name() {}
        Ok((true, ctx))
    }
}
