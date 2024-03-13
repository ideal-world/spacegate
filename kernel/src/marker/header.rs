use std::marker::PhantomData;

use hyper::{header::HeaderValue, Request};

use crate::{Marker, SgBody};

pub trait HeaderName {
    const NAME: &'static str;
}

pub struct Header<H>
where
    H: HeaderName,
{
    name: PhantomData<H>,
    pub value: HeaderValue,
}

impl<H> Header<H>
where
    H: HeaderName,
{
    pub fn new(value: impl Into<HeaderValue>) -> Self {
        Self {
            name: Default::default(),
            value: value.into(),
        }
    }
}

impl<H> Marker for Header<H>
where
    H: HeaderName + Send + Sync + 'static,
{
    fn extract(req: &Request<SgBody>) -> Option<Self> {
        req.headers().get(H::NAME).map(<Header<H>>::new)
    }

    fn attach(self, req: &mut Request<SgBody>) {
        req.headers_mut().insert(H::NAME, self.value.clone());
    }

    fn detach(req: &mut Request<SgBody>) -> Option<Self> {
        req.headers_mut().remove(H::NAME).map(<Header<H>>::new)
    }
}
