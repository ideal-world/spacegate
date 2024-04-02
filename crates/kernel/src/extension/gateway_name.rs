use std::{ops::Deref, sync::Arc};
#[derive(Debug, Clone)]
pub struct GatewayName(pub Arc<str>);

impl GatewayName {
    pub fn new(name: impl Into<Arc<str>>) -> Self {
        Self(name.into())
    }
}

impl Deref for GatewayName {
    type Target = str;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}
