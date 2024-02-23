use crate::service::backend::memory::Memory;

use super::{CreateListener, Listen};

struct Never;

impl Listen for Never {
    fn poll_next(&mut self, _cx: &mut std::task::Context<'_>) -> std::task::Poll<Option<(super::ConfigType, super::ConfigEventType)>> {
        std::task::Poll::Pending
    }
}

impl CreateListener for Memory {
    const CONFIG_LISTENER_NAME: &'static str = "memory";

    fn create_listener(&self) -> Result<Box<dyn super::Listen>, Box<dyn std::error::Error + Sync + Send + 'static>> {
        Ok(Box::new(Never))
    }
}
