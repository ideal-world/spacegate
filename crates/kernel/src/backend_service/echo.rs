use std::convert::Infallible;

use hyper::{Request, Response};

use tracing::instrument;

use crate::{extension::Reflect, SgBody, SgResponseExt};

#[instrument]
#[cold]
/// just return the body, you may use this service for test
pub async fn echo(mut req: Request<SgBody>) -> Result<Response<SgBody>, Infallible> {
    let reflect = req.extensions_mut().remove::<Reflect>();
    let body = req.into_body();

    let mut resp = Response::builder().body(body).unwrap_or_else(Response::bad_gateway);
    if let Some(reflect) = reflect {
        resp.extensions_mut().insert(reflect);
    }
    Ok(resp)
}
