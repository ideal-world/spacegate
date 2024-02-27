use crate::{service::backend::memory::Memory, Config};
use futures_util::FutureExt;
use std::sync::Arc;
use tokio::sync::RwLock;

use super::{CreateListener, Listen};
#[derive(Debug, Clone, Default)]
struct Static;

impl Listen for Static {
    fn poll_next(&mut self, _cx: &mut std::task::Context<'_>) -> std::task::Poll<Result<super::ListenEvent, crate::BoxError>> {
        std::task::Poll::Pending
    }
}

impl CreateListener for Memory {
    const CONFIG_LISTENER_NAME: &'static str = "memory";

    async fn create_listener(&self) -> Result<(Config, Box<dyn super::Listen>), Box<dyn std::error::Error + Sync + Send + 'static>> {
        Ok((self.config.as_ref().clone(), Box::new(Static)))
    }
}
