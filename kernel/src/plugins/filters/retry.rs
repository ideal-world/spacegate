use std::{sync::Arc, thread};

use async_trait::async_trait;
use hyper::Body;
use itertools::Itertools;
use lazy_static::lazy_static;
use serde::{Deserialize, Serialize};
use tardis::{
    basic::result::TardisResult,
    log,
    rand::{self, distributions::WeightedIndex, prelude::Distribution, thread_rng, Rng},
    tokio::sync::Mutex,
    TardisFuns,
};

use crate::{
    functions::{http_client, http_route::SgHttpRouteMatchInst},
    plugins::filters::retry::expiring_map::ExpireMap,
};

use super::{BoxSgPluginFilter, SgPluginFilter, SgPluginFilterDef, SgPluginFilterInitDto, SgRoutePluginContext};

lazy_static! {
    static ref REQUEST_BODY: Arc<Mutex<ExpireMap<Option<Vec<u8>>>>> = <_>::default();
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
    fn accept(&self) -> super::SgPluginFilterAccept {
        super::SgPluginFilterAccept {
            kind: vec![super::SgPluginFilterKind::Http],
            accept_error_response: true,
            ..Default::default()
        }
    }

    async fn init(&self, _: &SgPluginFilterInitDto) -> TardisResult<()> {
        Ok(())
    }

    async fn destroy(&self) -> TardisResult<()> {
        Ok(())
    }

    async fn req_filter(&self, _: &str, mut ctx: SgRoutePluginContext, _matched_match_inst: Option<&SgHttpRouteMatchInst>) -> TardisResult<(bool, SgRoutePluginContext)> {
        let mut req_body_cache = REQUEST_BODY.lock().await;
        let req_body = ctx.pop_req_body().await?;
        req_body_cache.insert(ctx.get_request_id().to_string(), req_body.clone(), (self.retries as u64 * self.max_interval) as u128);
        if let Some(req_body) = req_body {
            ctx.set_req_body(req_body)?;
        }
        Ok((true, ctx))
    }

    async fn resp_filter(&self, _: &str, mut ctx: SgRoutePluginContext, _: Option<&SgHttpRouteMatchInst>) -> TardisResult<(bool, SgRoutePluginContext)> {
        if ctx.is_resp_error() {
            let mut req_body_cache = REQUEST_BODY.lock().await;
            let req_body = req_body_cache.remove(ctx.get_request_id()).flatten();
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
                log::trace!("[SG.Filter.Retry] retry request retry_times:{} next_retry_backoff:{}", retry_count, backoff_interval);
                match http_client::raw_request(
                    None,
                    ctx.get_req_method().clone(),
                    &choose_backend_url(&mut ctx),
                    req_body.clone().map(Body::from),
                    ctx.get_req_headers(),
                    time_out,
                )
                .await
                {
                    Ok(response) => {
                        ctx = ctx.resp(response.status(), response.headers().clone(), response.into_body());
                        break;
                    }
                    Err(e) => ctx = ctx.resp_from_error(e),
                };
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

//TODO fix: Severe impact on performance .\
// It is possible that high concurrency may drag down performance (Above 500k QPS)
mod expiring_map {
    use std::collections::{HashMap, VecDeque};

    use tardis::chrono::Utc;

    /// Expiration Unit of time is milliseconds
    pub struct ExpireMap<V, K = String> {
        base: HashMap<K, V>,
        expire_time: VecDeque<(K, u128)>,
    }

    impl<V> Default for ExpireMap<V, String> {
        fn default() -> Self {
            Self {
                base: Default::default(),
                expire_time: Default::default(),
            }
        }
    }

    #[allow(dead_code)]
    impl<V> ExpireMap<V, String> {
        pub fn remove(&mut self, k: &str) -> Option<V> {
            self.remove_expired_items();
            self.base.remove(k)
        }

        pub fn insert(&mut self, k: String, v: V, millis: u128) -> Option<V> {
            let expire = millis + Utc::now().timestamp_millis() as u128;
            let idx = self.expire_time.partition_point(|(_, x)| x < &expire);
            self.expire_time.insert(idx, (k.clone(), expire));
            self.base.insert(k, v)
        }

        pub fn remove_expired_items(&mut self) {
            let now = Utc::now().timestamp_millis() as u128;
            while let Some((k, expire)) = self.expire_time.front() {
                if *expire <= now {
                    self.base.remove(k);
                    self.expire_time.pop_front();
                } else {
                    break;
                }
            }
        }
        fn get(&mut self, k: &str) -> Option<&V> {
            self.remove_expired_items();
            self.base.get(k)
        }
        fn len(&mut self) -> usize {
            self.remove_expired_items();
            self.base.len()
        }

        fn new() -> Self {
            Self {
                base: HashMap::new(),
                expire_time: VecDeque::new(),
            }
        }
    }

    #[cfg(test)]
    #[allow(clippy::unwrap_used)]
    mod tests {

        use super::ExpireMap;
        #[test]
        fn test() {
            let mut expire_map = ExpireMap::<Option<Vec<u8>>>::new();
            expire_map.insert("a".to_string(), Some(vec![1, 2, 3]), std::time::Duration::from_secs(1).as_millis());
            expire_map.insert("b".to_string(), Some(vec![1, 2, 3]), std::time::Duration::from_secs(1).as_millis());
            expire_map.insert("c".to_string(), Some(vec![1, 2, 3]), std::time::Duration::from_secs(2).as_millis());
            expire_map.insert("d".to_string(), Some(vec![1, 2, 3]), std::time::Duration::from_secs(3).as_millis());
            assert_eq!(expire_map.len(), 4);

            std::thread::sleep(std::time::Duration::from_secs(2));

            assert!(expire_map.get("a").is_none());
            assert!(expire_map.remove("a").is_none());
            assert_eq!(expire_map.len(), 1);

            let mut expire_map = ExpireMap::<Option<Vec<u8>>>::new();
            for i in 0..50 {
                let mut a = vec![1, 2, 3];
                for _ in 0..i {
                    a.push(1)
                }
                expire_map.insert(format!("a{}", i), Some(a), 1);
            }
            expire_map.insert("b".to_string(), Some(vec![1, 2, 3]), 4);
            expire_map.insert("c".to_string(), Some(vec![1, 2, 3]), 2);
            expire_map.insert("d".to_string(), Some(vec![1, 2, 3]), 5);

            std::thread::sleep(std::time::Duration::from_millis(2));

            assert!(expire_map.get("a").is_none());
            assert!(expire_map.remove("c").is_none());
            assert_eq!(expire_map.len(), 2);
        }
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use crate::functions::http_client;
    use http::{HeaderMap, Method, Uri, Version};
    use hyper::Body;
    use tardis::{basic::error::TardisError, tokio};

    use crate::plugins::{context::SgRoutePluginContext, filters::SgPluginFilter};

    use super::SgFilterRetry;

    #[tokio::test]
    async fn test_retry() {
        let filter_retry = SgFilterRetry { ..Default::default() };
        http_client::init().unwrap();
        let ctx = SgRoutePluginContext::new_http(
            Method::GET,
            Uri::from_static("http://sg.idealworld.group/iam/ct/001?name=sg"),
            Version::HTTP_11,
            HeaderMap::new(),
            Body::from(""),
            "127.0.0.1:8080".parse().unwrap(),
            "test_gate".to_string(),
            None,
        );
        let ctx = ctx.resp_from_error(TardisError::internal_error("", ""));

        filter_retry.resp_filter("", ctx, None).await.unwrap();
    }
}
