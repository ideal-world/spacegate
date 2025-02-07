use super::{Redis, RedisConfEvent, CONF_EVENT_CHANNEL};
use crate::{
    service::{config_format::ConfigFormat, ConfigEventType, ConfigType, CreateListener, Listen, ListenEvent, Retrieve as _},
    Config,
};
use futures_util::StreamExt;
use lru::LruCache;
use std::{num::NonZeroUsize, task::ready};
use tracing::error;

use lazy_static::lazy_static;
use tokio::sync::Mutex;

lazy_static! {
    static ref CHANGE_CACHE: Mutex<LruCache<String, bool>> = Mutex::new(LruCache::new(NonZeroUsize::new(100).expect("NonZeroUsize::new failed")));
}

pub struct RedisListener {
    rx: tokio::sync::mpsc::UnboundedReceiver<(ConfigType, ConfigEventType)>,
}

impl<F> CreateListener for Redis<F>
where
    F: ConfigFormat + Clone + Send + Sync + 'static,
{
    const CONFIG_LISTENER_NAME: &'static str = "file";
    type Listener = RedisListener;
    async fn create_listener(&self) -> Result<(Config, Self::Listener), Box<dyn std::error::Error + Sync + Send + 'static>> {
        let config = self.retrieve_config().await?;

        let (evt_tx, rx) = tokio::sync::mpsc::unbounded_channel();

        let mut pubsub = redis::Client::open(self.param.clone())?.get_async_connection().await?.into_pubsub();
        pubsub.subscribe(CONF_EVENT_CHANNEL).await?;
        tokio::spawn(async move {
            let mut stream = pubsub.on_message();
            while let Some(msg) = stream.next().await {
                let Ok(evt) = msg.get_payload::<RedisConfEvent>() else {
                    error!("parse redis event failed: {:?}", msg);
                    continue;
                };
                if let Err(e) = evt_tx.send((evt.0, evt.1)) {
                    error!("send redis event failed: {:?}", e);
                    return;
                }
            }
        });

        Ok((config, RedisListener { rx }))
    }
}

impl Listen for RedisListener {
    fn poll_next(&mut self, cx: &mut std::task::Context<'_>) -> std::task::Poll<Result<ListenEvent, crate::BoxError>> {
        if let Some(next) = ready!(self.rx.poll_recv(cx)) {
            std::task::Poll::Ready(Ok(next.into()))
        } else {
            std::task::Poll::Pending
        }
    }
}
