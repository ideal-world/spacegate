use std::pin::Pin;

use futures_util::Future;
use hyper::{Request, Response};

use crate::{SgBody, SgResponseExt};

use super::AsyncFilter;

#[derive(Debug, Clone, Copy)]
pub struct Dump;

impl AsyncFilter for Dump {
    type Future = Pin<Box<dyn Future<Output = Result<Request<SgBody>, Response<SgBody>>> + Send + 'static>>;
    fn filter(&self, req: Request<SgBody>) -> Self::Future {
        let (part, body) = req.into_parts();
        Box::pin(async move {
            let body = body.dump().await.map_err(|e| Response::<SgBody>::internal_error(e.as_ref()))?;
            Ok(Request::from_parts(part, body))
        })
    }
}
