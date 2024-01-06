use std::{convert::Infallible, future::Future, pin::Pin, sync::Arc, task::ready, time::Duration};

use hyper::{Request, Response};
use pin_project_lite::pin_project;
use serde::{Deserialize, Serialize};
use tardis::{
    rand::{self, Rng},
    tokio::{self, time::Sleep},
};
use tower::retry::{Policy, Retry as TowerRetry, RetryLayer as TowerRetryLayer};
use tower_layer::Layer;

use spacegate_tower::{
    helper_layers::async_filter::{dump::Dump, AsyncFilterRequest, AsyncFilterRequestLayer},
    SgBody, SgBoxLayer,
};

use crate::{def_plugin, MakeSgLayer};

pub struct RetryLayer {
    inner_layer: TowerRetryLayer<RetryPolicy>,
}

impl RetryLayer {
    pub fn new(policy: RetryPolicy) -> Self {
        Self {
            inner_layer: TowerRetryLayer::new(policy),
        }
    }
}

impl<S> Layer<S> for RetryLayer {
    type Service = AsyncFilterRequest<Dump, TowerRetry<RetryPolicy, S>>;

    fn layer(&self, service: S) -> Self::Service {
        AsyncFilterRequestLayer::new(Dump).layer(self.inner_layer.layer(service))
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

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(default)]
pub struct RetryConfig {
    pub retries: u16,
    pub retirable_methods: Vec<String>,
    /// Backoff strategies can vary depending on the specific implementation and requirements.
    /// see [BackOff]
    pub backoff: BackOff,
    /// milliseconds
    pub base_interval: u64,
    /// milliseconds
    pub max_interval: u64,
}

impl Default for RetryConfig {
    fn default() -> Self {
        Self {
            retries: 3,
            retirable_methods: vec!["*".to_string()],
            backoff: BackOff::default(),
            base_interval: 100,
            //10 seconds
            max_interval: 10000,
        }
    }
}

#[derive(Clone)]
pub struct RetryPolicy {
    times: usize,
    config: Arc<RetryConfig>,
}
pin_project! {
    pub struct Delay<T> {
        value: Option<T>,
        #[pin]
        sleep: Sleep,
    }
}

impl<T> Delay<T> {
    pub fn new(value: T, duration: Duration) -> Self {
        Self {
            value: Some(value),
            sleep: tokio::time::sleep(duration),
        }
    }
}

impl<T> Future for Delay<T> {
    type Output = T;

    fn poll(self: Pin<&mut Self>, cx: &mut std::task::Context<'_>) -> std::task::Poll<Self::Output> {
        let this = self.project();
        ready!(this.sleep.poll(cx));
        std::task::Poll::Ready(this.value.take().expect("poll after ready"))
    }
}

impl Policy<Request<SgBody>, Response<SgBody>, Infallible> for RetryPolicy {
    type Future = Delay<Self>;

    fn retry(&self, _req: &Request<SgBody>, result: Result<&Response<SgBody>, &Infallible>) -> Option<Self::Future> {
        if self.times < self.config.retries.into() && result.is_err() {
            let delay = match self.config.backoff {
                BackOff::Fixed => self.config.base_interval,
                BackOff::Exponential => self.config.base_interval * 2u64.pow(self.times as u32),
                BackOff::Random => {
                    let mut rng = rand::thread_rng();
                    rng.gen_range(self.config.base_interval..self.config.max_interval)
                }
            };
            Some(Delay::new(
                RetryPolicy {
                    times: self.times + 1,
                    config: self.config.clone(),
                },
                Duration::from_millis(delay),
            ))
        } else {
            None
        }
    }

    fn clone_request(&self, req: &Request<SgBody>) -> Option<Request<SgBody>> {
        if !req.body().is_dumped() {
            Some(req.clone())
        } else {
            None
        }
    }
}

impl MakeSgLayer for RetryConfig {
    fn make_layer(&self) -> Result<spacegate_tower::SgBoxLayer, tower::BoxError> {
        let policy = RetryPolicy {
            times: 0,
            config: Arc::new(self.clone()),
        };
        let layer = RetryLayer::new(policy);
        Ok(SgBoxLayer::new(layer))
    }
}

def_plugin!("retry", RetryPlugin, RetryConfig);
