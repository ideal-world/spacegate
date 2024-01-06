use std::convert::Infallible;

use hyper::{Request, Response};

use tracing::instrument;

use crate::{extension::Reflect, SgBody, SgResponseExt};

#[instrument]
pub async fn echo(mut req: Request<SgBody>) -> Result<Response<SgBody>, Infallible> {
    let reflect = req.extensions_mut().remove::<Reflect>();
    let body = req.into_body();

    let mut resp = Response::builder().body(body).unwrap_or_else(Response::internal_error);
    if let Some(reflect) = reflect {
        resp.extensions_mut().insert(reflect);
    }
    Ok(resp)
}
