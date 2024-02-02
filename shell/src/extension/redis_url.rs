use std::sync::Arc;
#[derive(Debug, Clone)]
pub struct RedisUrl(pub Arc<str>);
