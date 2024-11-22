pub mod basic;
pub mod bearer;
use hyper::{header::AUTHORIZATION, http::HeaderValue};

use crate::{extractor::OptionalExtract, injector::Inject, BoxError, BoxResult, SgRequest};
#[derive(Debug, Clone)]
pub struct Authorization<A>(pub A);

impl<A> Authorization<A> {
    pub fn new(auth: A) -> Self {
        Self(auth)
    }
}

impl<A> OptionalExtract for Authorization<A>
where
    A: TryFrom<HeaderValue> + Send + Sync + 'static,
    A::Error: Into<BoxError>,
{
    fn extract(req: &SgRequest) -> Option<Self> {
        let auth = req.headers().get(AUTHORIZATION)?.clone();
        let auth = A::try_from(auth).ok()?;
        Some(Self(auth))
    }
}

impl<A> Inject for Authorization<A>
where
    for<'a> &'a A: TryInto<HeaderValue> + Send + Sync,
    for<'a> <&'a A as TryInto<HeaderValue>>::Error: Into<BoxError>,
{
    fn inject(&self, req: &mut SgRequest) -> BoxResult<()> {
        req.headers_mut().insert(AUTHORIZATION, (&self.0).try_into().map_err(Into::into)?);
        Ok(())
    }
}
