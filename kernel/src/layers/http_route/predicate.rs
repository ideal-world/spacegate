use std::sync::Arc;

use hyper::{Request, Response, StatusCode};

use crate::helper_layers::filter;
use crate::SgResponseExt;
use crate::{ReqOrResp, SgBody};

#[derive(Debug, Clone)]
pub struct FilterByHostnames {
    pub hostnames: Arc<[String]>,
}

impl FilterByHostnames {
    pub fn check(&self, request: Request<SgBody>) -> ReqOrResp {
        if self.hostnames.is_empty() {
            Ok(request)
        } else {
            let hostname = request.uri().host();
            if let Some(hostname) = hostname {
                if self.hostnames.iter().any(|h| h == hostname) {
                    Ok(request)
                } else {
                    Err(Response::<SgBody>::with_code_message(StatusCode::FORBIDDEN, "hostname not allowed"))
                }
            } else {
                Err(Response::<SgBody>::with_code_message(StatusCode::FORBIDDEN, "missing hostname"))
            }
        }
    }
}

impl filter::Filter for FilterByHostnames {
    fn filter(&self, req: Request<SgBody>) -> Result<Request<SgBody>, Response<SgBody>> {
        FilterByHostnames::check(self, req)
    }
}
