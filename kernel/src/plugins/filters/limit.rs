use std::time::SystemTime;

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use tardis::{
    basic::{error::TardisError, result::TardisResult},
    cache::Script,
    TardisFuns,
};

use crate::functions::http_route::SgHttpRouteMatchInst;

use super::{BoxSgPluginFilter, SgPluginFilter, SgPluginFilterDef, SgRouteFilterContext};
use lazy_static::lazy_static;
pub const CODE: &str = "limit";

pub struct SgFilterLimitDef;

impl SgPluginFilterDef for SgFilterLimitDef {
    fn inst(&self, spec: serde_json::Value) -> TardisResult<BoxSgPluginFilter> {
        let filter = TardisFuns::json.json_to_obj::<SgFilterLimit>(spec)?;
        Ok(filter.boxed())
    }
}

#[derive(Default, Debug, Serialize, Deserialize, Clone)]
pub struct SgFilterLimit {
    pub max_request_number: Option<u64>,
    pub time_window_ms: Option<u64>,
}

const CONF_LIMIT_KEY: &str = "sg:plugin:filter:limit:";

lazy_static! {
    /// Flow limit script
    ///
    /// # Arguments
    ///
    /// * KEYS[1]  counter key
    /// * KEYS[2]  last counter reset timestamp key
    /// * ARGV[1]  maximum number of request
    /// * ARGV[2]  time window
    /// * ARGV[3]  current timestamp
    ///
    /// # Return
    ///
    /// * 1   passed
    /// * 0   limited
    ///
    /// # Kernel logic
    ///
    /// ```lua
    /// -- Use `counter` to accumulate 1 for each request
    /// local current_count = tonumber(redis.call('incr', KEYS[1]));
    /// if current_count == 1 then
    ///     -- The current request is the first request, record the current timestamp
    ///     redis.call('set', KEYS[2], ARGV[3]);
    /// end
    /// -- When the `counter` value reaches the maximum number of requests
    /// if current_count > tonumber(ARGV[1]) then
    ///     local last_refresh_time = tonumber(redis.call('get', KEYS[2]));
    ///     if last_refresh_time + tonumber(ARGV[2]) > tonumber(ARGV[3]) then
    ///          -- Last reset time + time window > current time,
    ///          -- indicating that the request has reached the upper limit within this time period,
    ///          -- so the request is limited
    ///         return 0;
    ///     end
    ///     -- Otherwise reset the counter and timestamp,
    ///     -- and allow the request
    ///     redis.call('set', KEYS[1], '1')
    ///     redis.call('set', KEYS[2], ARGV[3]);
    /// end
    /// return 1;
    /// ```
    static ref SCRIPT: Script = Script::new(
        r"
    local current_count = tonumber(redis.call('incr', KEYS[1]));
    if current_count == 1 then
        redis.call('set', KEYS[2], ARGV[3]);
    end
    if current_count > tonumber(ARGV[1]) then
        local last_refresh_time = tonumber(redis.call('get', KEYS[2]));
        if last_refresh_time + tonumber(ARGV[2]) > tonumber(ARGV[3]) then
            return 0;
        end
        redis.call('set', KEYS[1], '1')
        redis.call('set', KEYS[2], ARGV[3]);
    end
    return 1;
    ",
    );
}

#[async_trait]
impl SgPluginFilter for SgFilterLimit {
    fn kind(&self) -> super::SgPluginFilterKind {
        super::SgPluginFilterKind::Http
    }

    async fn init(&self) -> TardisResult<()> {
        Ok(())
    }

    async fn destroy(&self) -> TardisResult<()> {
        Ok(())
    }

    async fn req_filter(&self, id: &str, ctx: SgRouteFilterContext, _: Option<&SgHttpRouteMatchInst>) -> TardisResult<(bool, SgRouteFilterContext)> {
        if let Some(max_request_number) = &self.max_request_number {
            let result: &bool = &SCRIPT
                // counter key
                .key(format!("{CONF_LIMIT_KEY}{id}"))
                // last counter reset timestamp key
                .key(format!("{CONF_LIMIT_KEY}{id}_ts"))
                // maximum number of request
                .arg(max_request_number)
                // time window
                .arg(self.time_window_ms.unwrap_or(1000))
                // current timestamp
                .arg(SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_millis() as u64)
                .invoke_async(&mut ctx.cache()?.cmd().await?)
                .await
                .unwrap();
            if !result {
                return Err(TardisError::forbidden("[SG.Filter.Limit] too many requests", ""));
            }
        }
        Ok((true, ctx))
    }

    async fn resp_filter(&self, _: &str, ctx: SgRouteFilterContext, _: Option<&SgHttpRouteMatchInst>) -> TardisResult<(bool, SgRouteFilterContext)> {
        Ok((true, ctx))
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use crate::functions::cache_client;

    use super::*;
    use http::{HeaderMap, Method, Uri, Version};
    use hyper::Body;
    use tardis::{
        test::test_container::TardisTestContainer,
        testcontainers,
        tokio::{self, time::sleep},
    };

    #[tokio::test]
    async fn test_limit_filter() {
        let docker = testcontainers::clients::Cli::default();
        let redis_container = TardisTestContainer::redis_custom(&docker);
        let port = redis_container.get_host_port_ipv4(6379);
        let url = format!("redis://127.0.0.1:{port}/0",);
        cache_client::init("test_gate", &url).await.unwrap();

        let filter = SgFilterLimit {
            max_request_number: Some(4),
            ..Default::default()
        };

        fn new_ctx() -> SgRouteFilterContext {
            SgRouteFilterContext::new(
                Method::GET,
                Uri::from_static("http://sg.idealworld.group/iam/ct/001?name=sg"),
                Version::HTTP_11,
                HeaderMap::new(),
                Body::empty(),
                "127.0.0.1:8080".parse().unwrap(),
                "test_gate".to_string(),
            )
        }

        assert!(filter.req_filter("limit_001", new_ctx(), None).await.is_ok());
        assert!(filter.req_filter("limit_001", new_ctx(), None).await.is_ok());
        assert!(filter.req_filter("limit_001", new_ctx(), None).await.is_ok());
        assert!(filter.req_filter("limit_001", new_ctx(), None).await.is_ok());
        assert!(filter.req_filter("limit_001", new_ctx(), None).await.is_err());
        assert!(filter.req_filter("limit_002", new_ctx(), None).await.is_ok());
        assert!(filter.req_filter("limit_001", new_ctx(), None).await.is_err());

        sleep(Duration::from_millis(1100)).await;

        assert!(filter.req_filter("limit_001", new_ctx(), None).await.is_ok());
        assert!(filter.req_filter("limit_001", new_ctx(), None).await.is_ok());
        assert!(filter.req_filter("limit_001", new_ctx(), None).await.is_ok());
        assert!(filter.req_filter("limit_001", new_ctx(), None).await.is_ok());
        assert!(filter.req_filter("limit_001", new_ctx(), None).await.is_err());
        assert!(filter.req_filter("limit_001", new_ctx(), None).await.is_err());

        sleep(Duration::from_millis(1100)).await;

        assert!(filter.req_filter("limit_001", new_ctx(), None).await.is_ok());
        assert!(filter.req_filter("limit_001", new_ctx(), None).await.is_ok());
        assert!(filter.req_filter("limit_001", new_ctx(), None).await.is_ok());
        assert!(filter.req_filter("limit_001", new_ctx(), None).await.is_ok());
        assert!(filter.req_filter("limit_001", new_ctx(), None).await.is_err());
        assert!(filter.req_filter("limit_001", new_ctx(), None).await.is_err());
    }
}
