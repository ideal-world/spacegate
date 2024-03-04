use lru::LruCache;
use redis::{AsyncCommands, AsyncIter};
use std::{num::NonZeroUsize, task::ready};

use crate::{
    service::{
        backend::redis::{Redis, CONF_CHANGE_TRIGGER},
        config_format::ConfigFormat,
        Retrieve as _,
    },
    Config,
};

use super::{ConfigEventType, ConfigType, CreateListener, Listen};
use lazy_static::lazy_static;
use std::time::Duration;
use tokio::{sync::Mutex, time};

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

    async fn create_listener(&self) -> Result<(Config, Box<dyn super::Listen>), Box<dyn std::error::Error + Sync + Send + 'static>> {
        let config = self.retrieve_config().await?;

        let (evt_tx, rx) = tokio::sync::mpsc::unbounded_channel();

        let mut redis_con = self.get_con().await.expect("[SG.Config] cache_client get_con failed");

        tokio::spawn(async move {
            let mut interval = time::interval(Duration::from_secs(1));
            loop {
                {
                    tracing::trace!("[SG.Config] Config change check");
                    let mut key_iter: AsyncIter<String> = redis_con.scan_match(&format!("{}*", CONF_CHANGE_TRIGGER)).await.expect("[SG.Config] cache_client scan_match failed");

                    while let Some(changed_key) = key_iter.next_item().await {
                        let changed_key = changed_key.strip_prefix(CONF_CHANGE_TRIGGER).expect("[SG.Config] strip_prefix failed");
                        let f = changed_key.split("##").collect::<Vec<_>>();
                        let unique = f[0];
                        let mut lock = CHANGE_CACHE.lock().await;
                        if lock.put(unique.to_string(), true).is_some() {
                            continue;
                        }
                        let changed_obj = f[1];
                        let changed_method = f[2];
                        let changed_gateway_name = f[3];
                        tracing::trace!("[SG.Config] Config change found, {changed_obj}:[{changed_method}] {changed_gateway_name}");

                        let send_tx = |config: ConfigType, type_: ConfigEventType| {
                            evt_tx.send((config, type_)).expect("[SG.Config] send failed");
                        };
                        match changed_method {
                            "create" => match changed_obj {
                                "gateway" => {
                                    send_tx(
                                        ConfigType::Gateway {
                                            name: changed_gateway_name.to_string(),
                                        },
                                        ConfigEventType::Create,
                                    );
                                }
                                "httproute" => send_tx(
                                    ConfigType::Route {
                                        gateway_name: changed_gateway_name.to_string(),
                                        name: f[4].to_string(),
                                    },
                                    ConfigEventType::Create,
                                ),
                                _ => {}
                            },
                            "update" => match changed_obj {
                                "gateway" => {
                                    send_tx(
                                        ConfigType::Gateway {
                                            name: changed_gateway_name.to_string(),
                                        },
                                        ConfigEventType::Update,
                                    );
                                }
                                "httproute" => send_tx(
                                    ConfigType::Route {
                                        gateway_name: changed_gateway_name.to_string(),
                                        name: f[4].to_string(),
                                    },
                                    ConfigEventType::Update,
                                ),
                                _ => {}
                            },
                            "delete" => match changed_obj {
                                "gateway" => {
                                    send_tx(
                                        ConfigType::Gateway {
                                            name: changed_gateway_name.to_string(),
                                        },
                                        ConfigEventType::Delete,
                                    );
                                }
                                "httproute" => send_tx(
                                    ConfigType::Route {
                                        gateway_name: changed_gateway_name.to_string(),
                                        name: f[4].to_string(),
                                    },
                                    ConfigEventType::Delete,
                                ),
                                _ => {}
                            },
                            _ => {}
                        }
                    }
                }
                interval.tick().await;
            }
        });

        Ok((config, Box::new(RedisListener { rx })))
    }
}

impl Listen for RedisListener {
    fn poll_next(&mut self, cx: &mut std::task::Context<'_>) -> std::task::Poll<Result<super::ListenEvent, crate::BoxError>> {
        if let Some(next) = ready!(self.rx.poll_recv(cx)) {
            std::task::Poll::Ready(Ok(next))
        } else {
            std::task::Poll::Pending
        }
    }
}
