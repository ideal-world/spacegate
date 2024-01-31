use std::{ops::Deref, sync::Arc};
#[derive(Debug, Clone)]
pub struct BackendHost(pub Arc<str>);

impl BackendHost {
    pub fn new(name: impl Into<Arc<str>>) -> Self {
        Self(name.into())
    }
}

impl Deref for BackendHost {
    type Target = str;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}
