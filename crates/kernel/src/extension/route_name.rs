use std::{ops::Deref, sync::Arc};

#[derive(Debug, Clone)]
pub struct RouteName(pub Arc<str>);

impl RouteName {
    pub fn new(name: impl Into<Arc<str>>) -> Self {
        Self(name.into())
    }
}

impl Deref for RouteName {
    type Target = str;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}
