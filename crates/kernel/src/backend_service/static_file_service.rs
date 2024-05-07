use std::{fs::Metadata, os::unix::fs::MetadataExt, path::Path};

use chrono::{DateTime, Utc};
use hyper::{
    header::{HeaderValue, CONTENT_TYPE, IF_MODIFIED_SINCE, IF_UNMODIFIED_SINCE, LOCATION},
    HeaderMap, Response, StatusCode,
};
use tokio::io::AsyncReadExt;
use tracing::{instrument, trace};

use crate::{extension::Reflect, SgBody, SgRequest, SgResponse};

fn header_value_to_rfc3339(header: &HeaderValue) -> Option<DateTime<Utc>> {
    let header = header.to_str().ok()?;
    Some(DateTime::parse_from_rfc3339(header).ok()?.to_utc())
}
fn predict(headers: &HeaderMap, last_modified: Option<DateTime<Utc>>) -> Option<StatusCode> {
    if let Some(since) = headers.get(IF_UNMODIFIED_SINCE).and_then(header_value_to_rfc3339) {
        if let Some(last_modified) = last_modified {
            if last_modified > since {
                return Some(StatusCode::PRECONDITION_FAILED);
            }
        }
    }
    if let Some(since) = headers.get(IF_MODIFIED_SINCE).and_then(header_value_to_rfc3339) {
        if let Some(last_modified) = last_modified {
            if last_modified <= since {
                return Some(StatusCode::NOT_MODIFIED);
            }
        }
    }
    None
}

// temporary implementation
pub fn cache_policy(metadata: &Metadata) -> bool {
    let size = metadata.size();
    // cache file less than 1MB
    size < (1 << 20)
}

#[instrument()]
pub async fn static_file_service(mut request: SgRequest, dir: &Path) -> SgResponse {
    let mut response = Response::builder().body(SgBody::empty()).expect("failed to build response");
    if let Some(reflect) = request.extensions_mut().remove::<Reflect>() {
        *response.extensions_mut() = reflect.into_inner();
    }
    let Ok(dir) = dir.canonicalize() else {
        *response.body_mut() = SgBody::full(format!("cannot canonicalize dir path {dir:?}"));
        *response.status_mut() = StatusCode::FORBIDDEN;
        return response;
    };

    let Ok(path) = dir.join(request.uri().path().trim_start_matches('/')).canonicalize() else {
        *response.body_mut() = SgBody::full("cannot canonicalize file path");
        *response.status_mut() = StatusCode::FORBIDDEN;
        return response;
    };
    trace!("static file path: {:?}", path);
    if !path.starts_with(dir) {
        *response.body_mut() = SgBody::full("file is not under the path");
        *response.status_mut() = StatusCode::FORBIDDEN;
        return response;
    }
    let mut file = match tokio::fs::File::open(&path).await {
        Ok(file) => file,
        Err(e) => match e.kind() {
            std::io::ErrorKind::NotFound => {
                *response.status_mut() = StatusCode::NOT_FOUND;
                return response;
            }
            std::io::ErrorKind::PermissionDenied => {
                *response.body_mut() = SgBody::full("access permission denied");
                *response.status_mut() = StatusCode::FORBIDDEN;
                return response;
            }
            e => {
                tracing::error!("failed to read file: {:?}", e);
                *response.status_mut() = StatusCode::INTERNAL_SERVER_ERROR;
                return response;
            }
        },
    };

    if let Ok(metadata) = file.metadata().await {
        let last_modified: Option<DateTime<Utc>> = metadata.modified().ok().map(|t| t.into());
        if let Some(code) = predict(request.headers(), last_modified) {
            *response.status_mut() = code;
            return response;
        }
        if metadata.is_dir() {
            // we may return dir page in the future
            *response.status_mut() = StatusCode::SEE_OTHER;
            // redirect to index.html
            response.headers_mut().insert(LOCATION, HeaderValue::from_static("/index.html"));
            return response;
        }
        let cache_this = cache_policy(&metadata);
        if cache_this {
            // todo: cache
        }
    }
    let mimes = mime_guess::from_path(path).into_iter().filter_map(|mime| HeaderValue::from_str(mime.essence_str()).ok());
    for mime_value in mimes {
        response.headers_mut().append(CONTENT_TYPE, mime_value);
    }
    let mut buffer = Vec::new();
    let _read = file.read_to_end(&mut buffer).await;
    *response.status_mut() = StatusCode::OK;
    *response.body_mut() = SgBody::full(buffer);
    response
}
