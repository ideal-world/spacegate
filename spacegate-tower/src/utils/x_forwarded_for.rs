use hyper::{header::HeaderValue, Request};
use tower::BoxError;

use crate::{extension::PeerAddr, header::X_FORWARDED_FOR, SgBody};

/// Add `x-forwarded-for` for request, based on [PeerAddr](`crate::extension::PeerAddr`)
pub fn x_forwarded_for(req: &mut Request<SgBody>) -> Result<(), BoxError> {
    let peer_ip = req.extensions().get::<PeerAddr>().ok_or(BoxError::from("missing peer addr ext"))?.0.ip();
    // add x-forward-for header
    req.headers_mut().append(X_FORWARDED_FOR, HeaderValue::from_str(&peer_ip.to_string())?);
    Ok(())
}
