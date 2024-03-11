use axum::{http::StatusCode, response::{IntoResponse, Response}};
use spacegate_config::BoxError;

pub struct InternalError<E>(pub E);
impl IntoResponse for InternalError<BoxError> {
    fn into_response(self) -> Response {
        let body = axum::body::Body::from(format!("Internal error: {}", self.0));
        Response::builder().status(StatusCode::INTERNAL_SERVER_ERROR).body(body).unwrap()
    }
}