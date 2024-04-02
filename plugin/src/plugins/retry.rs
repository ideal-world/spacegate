use std::{convert::Infallible, future::Future, pin::Pin, sync::Arc, task::ready, time::Duration};

use http_body_util::BodyExt;
use hyper::{Request, Response, StatusCode};
use pin_project_lite::pin_project;
use rand::Rng;
use serde::{Deserialize, Serialize};
use tokio::time::Sleep;
use tower_layer::Layer;

use spacegate_kernel::{SgBody, SgBoxLayer, SgResponseExt};

use crate::def_plugin;
#[derive(Debug, Clone)]
pub struct RetryLayer<P> {
    policy_default: P,
}

impl<P> RetryLayer<P>
where
    P: Policy,
{
    pub fn new(policy_default: P) -> Self {
        Self { policy_default }
    }
}

impl<S, P> Layer<S> for RetryLayer<P>
where
    P: Clone,
{
    type Service = Retry<P, S>;

    fn layer(&self, service: S) -> Self::Service {
        Retry {
            policy: self.policy_default.clone(),
            service,
        }
    }
}

#[derive(Debug, Default, Serialize, Deserialize, Clone)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
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
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(default)]
pub struct SgPluginRetryConfig {
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

impl Default for SgPluginRetryConfig {
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
    config: Arc<SgPluginRetryConfig>,
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
pub trait Policy: Sized {
    /// The [`Future`] type returned by [`Policy::retry`].
    type Future: Future<Output = Self>;

    fn retry(&self, req: &Request<SgBody>, response: &Response<SgBody>) -> Option<Self::Future>;
}

impl Policy for RetryPolicy {
    type Future = Delay<Self>;

    fn retry(&self, _req: &Request<SgBody>, response: &Response<SgBody>) -> Option<Self::Future> {
        if self.times < self.config.retries.into() && response.status() == StatusCode::INTERNAL_SERVER_ERROR {
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
}

#[derive(Debug, Clone)]
pub struct Retry<P, S> {
    policy: P,
    service: S,
}

pin_project_lite::pin_project! {
    pub struct RetryFuture<P, S>
    where
        P: Policy,
        S: hyper::service::Service<Request<SgBody>, Response = Response<SgBody>, Error = Infallible>,
    {
        policy: P,
        service: S,
        #[pin]
        state: RetryState<P::Future, S::Future>,
        request: Option<Request<SgBody>>
    }
}

impl<P, S> RetryFuture<P, S>
where
    P: Policy,
    S: hyper::service::Service<Request<SgBody>, Response = Response<SgBody>, Error = Infallible>,
{
    pub fn new(policy: P, service: S, req: Request<SgBody>) -> Self {
        let (parts, body) = req.into_parts();
        let body = body.collect();
        Self {
            policy,
            service,
            state: RetryState::Collecting { body, parts },
            request: None,
        }
    }
}

pin_project_lite::pin_project! {
    #[project = RetryStateProj]
    pub enum RetryState<PF, SF> {
        Collecting {
            #[pin]
            body: http_body_util::combinators::Collect<SgBody>,
            parts: hyper::http::request::Parts,
        },
        Requesting {
            #[pin]
            future: SF,
        },
        Retrying {
            #[pin]
            future: PF,
        },

    }
}

impl<P, S> Future for RetryFuture<P, S>
where
    P: Policy,
    S: hyper::service::Service<Request<SgBody>, Response = Response<SgBody>, Error = Infallible>,
{
    type Output = Result<Response<SgBody>, Infallible>;

    fn poll(self: Pin<&mut Self>, cx: &mut std::task::Context<'_>) -> std::task::Poll<Self::Output> {
        let mut this = self.project();
        loop {
            match this.state.as_mut().project() {
                RetryStateProj::Collecting { body, parts: part } => {
                    let body = ready!(body.poll(cx));
                    match body {
                        Ok(body) => {
                            let req = Request::from_parts(part.clone(), SgBody::full(body.to_bytes()));
                            {
                                let req = req.clone();
                                *this.request = Some(req);
                            }
                            let fut = this.service.call(req);
                            this.state.set(RetryState::Requesting { future: fut });
                        }
                        Err(e) => {
                            return std::task::Poll::Ready(Ok(Response::with_code_message(StatusCode::BAD_REQUEST, e.to_string())));
                        }
                    }
                }
                RetryStateProj::Requesting { future } => {
                    let resp = ready!(future.poll(cx));
                    match resp {
                        Ok(resp) => {
                            if let Some(fut) = this.policy.retry(this.request.as_ref().expect("status conflict"), &resp) {
                                this.state.set(RetryState::Retrying { future: fut });
                            } else {
                                return std::task::Poll::Ready(Ok(resp));
                            }
                        }
                        Err(_e) => {
                            unreachable!()
                        }
                    }
                }
                RetryStateProj::Retrying { future } => {
                    let next_p = ready!(future.poll(cx));
                    *this.policy = next_p;
                    // retry
                    let fut = this.service.call(this.request.as_ref().expect("status conflict").clone());
                    this.state.set(RetryState::Requesting { future: fut });
                }
            }
        }
    }
}

impl<P, S> hyper::service::Service<Request<SgBody>> for Retry<P, S>
where
    P: Policy + Clone,
    S: hyper::service::Service<Request<SgBody>, Response = Response<SgBody>, Error = Infallible> + Clone,
{
    type Response = Response<SgBody>;

    type Error = Infallible;

    type Future = RetryFuture<P, S>;

    fn call(&self, req: Request<SgBody>) -> Self::Future {
        RetryFuture::new(self.policy.clone(), self.service.clone(), req)
    }
}

def_plugin!("retry", RetryPlugin, SgPluginRetryConfig; #[cfg(feature = "schema")] schema;);
#[cfg(feature = "schema")]
crate::schema! {
    RetryPlugin,
    SgPluginRetryConfig
}
