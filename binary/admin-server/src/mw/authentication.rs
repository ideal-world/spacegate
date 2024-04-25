use axum::{
    extract::{self, State},
    http::StatusCode,
    middleware::Next,
    response::Response,
};
use axum_extra::extract::cookie::CookieJar;
use jsonwebtoken::{decode, Algorithm, DecodingKey, Validation};
use serde::{Deserialize, Serialize};

use crate::state::AppState;

/// Our claims struct, it needs to derive `Serialize` and/or `Deserialize`
#[derive(Debug, Serialize, Deserialize)]
pub struct Claims {
    pub sub: String,
    pub exp: u64,
    pub username: String,
}

pub struct Authentication {
    pub secret: String,
}

pub async fn authentication<B>(State(state): State<AppState<B>>, cookie: CookieJar, request: extract::Request, next: Next) -> Response {
    use axum::http::header::AUTHORIZATION;
    if let Some(secret) = state.secret {
        let Some(jwt) = request
            .headers()
            .get(AUTHORIZATION)
            .and_then(|header| header.to_str().ok())
            .and_then(|header| header.strip_prefix("Bearer "))
            .or(cookie.get("jwt").map(|cookie| cookie.value()))
        else {
            return Response::builder().status(StatusCode::UNAUTHORIZED).body("expect jwt token".into()).unwrap();
        };
        let Ok(_jwt) = decode::<Claims>(jwt, &DecodingKey::from_secret(secret.as_ref()), &Validation::new(Algorithm::HS256)) else {
            return Response::builder().status(StatusCode::UNAUTHORIZED).body("invalid jwt token".into()).unwrap();
        };
    }

    next.run(request).await
}
