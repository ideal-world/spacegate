use hyper::{header::CONTENT_TYPE, Request, Response};

use crate::SgBody;

use super::Filter;

#[derive(Debug, Clone)]
pub struct ResponseAnyway {
    pub status: hyper::StatusCode,
    pub message: hyper::body::Bytes,
}

impl Filter for ResponseAnyway {
    fn filter(&self, _req: Request<SgBody>) -> Result<Request<SgBody>, Response<SgBody>> {
        Err(Response::builder().status(self.status).header(CONTENT_TYPE, "text/html; charset=utf-8").body(SgBody::full(self.message.clone())).expect("invalid response builder"))
    }
}
