use std::sync::Arc;

/// Extension to store current redis url
#[derive(Debug, Clone)]
pub struct RedisUrl(pub Arc<str>);
