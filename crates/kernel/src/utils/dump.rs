use crate::{BoxResult, SgRequest};

/// Dump the request body.
/// # Errors
/// 1. Fail to dump the body.
pub async fn dump(req: SgRequest) -> BoxResult<SgRequest> {
    if req.body().is_dumped() {
        Ok(req)
    } else {
        let (parts, body) = req.into_parts();
        let body = body.dump().await?;
        Ok(SgRequest::from_parts(parts, body))
    }
}
