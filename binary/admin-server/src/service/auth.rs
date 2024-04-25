use std::time::SystemTime;

use crate::{
    error::InternalError,
    mw::authentication::Claims,
    state::{self, AppState},
};
use axum::{
    extract::State,
    http::{header::SET_COOKIE, HeaderValue},
    routing::post,
    Json, Router,
};
use jsonwebtoken::{encode, EncodingKey, Header};
use serde::{Deserialize, Serialize};
#[derive(Debug, Serialize, Deserialize)]
pub struct Login {
    pub ak: String,
    pub sk: String,
}
const EXPIRE: u64 = 3600;
async fn login<B>(State(AppState { secret, sk_digest, .. }): State<AppState<B>>, login: Json<Login>) -> Result<axum::response::Response, InternalError> {
    let mut response = axum::response::Response::new(axum::body::Body::empty());
    if let Some(sk_digest) = sk_digest {
        let out: [u8; 32] = <sha2::Sha256 as digest::Digest>::digest(&login.sk).into();
        if &out != sk_digest.as_ref() {
            *response.status_mut() = axum::http::StatusCode::UNAUTHORIZED;
            return Ok(response);
        }
    }
    if let Some(sec) = secret {
        let jwt = encode(
            &Header::default(),
            &Claims {
                sub: "admin".to_string(),
                exp: SystemTime::now().duration_since(SystemTime::UNIX_EPOCH).unwrap().as_secs() + EXPIRE,
                username: login.ak.to_string(),
            },
            &EncodingKey::from_secret(sec.as_ref()),
        )
        .map_err(InternalError::boxed)?;
        response.headers_mut().insert(
            SET_COOKIE,
            HeaderValue::from_str(&format!("jwt={jwt}; path=/; HttpOnly; Max-Age=3600")).expect("invalid jwt"),
        );
    }
    Ok(response)
}
pub fn router<B: Send + Sync + 'static>() -> axum::Router<state::AppState<B>> {
    Router::new().route("/login", post(login::<B>))
}
