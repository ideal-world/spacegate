use hyper::Version;

use crate::SgRequest;

pub fn h2_downgrade(request: &mut SgRequest) {
    let version = request.version();
    if matches!(version, Version::HTTP_2) {
        *request.version_mut() = Version::HTTP_11;
    }
}
