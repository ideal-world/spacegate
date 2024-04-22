use crate::BoxError;
use hyper::{header::HeaderValue, Request};

use crate::{extension::PeerAddr, SgBody};
const X_FORWARDED_FOR: &str = "x-forwarded-for";
/// Add `x-forwarded-for` for request, based on [`PeerAddr`](`crate::extension::PeerAddr`)
/// # Errors
/// missing peer addr ext
pub fn x_forwarded_for(req: &mut Request<SgBody>) -> Result<(), BoxError> {
    if let Some(peer_ip) = req.extensions().get::<PeerAddr>().map(|x| x.0.ip()) {
        req.headers_mut().append(X_FORWARDED_FOR, HeaderValue::from_str(&peer_ip.to_string())?);
    }
    Ok(())
}
