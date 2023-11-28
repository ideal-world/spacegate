use k8s_openapi::http::StatusCode;

use kernel_common::client::k8s_client::DEFAULT_CLIENT_NAME;
use serde::{Deserialize, Serialize};
use tardis::web::poem::endpoint::BoxEndpoint;
use tardis::web::poem::session::{CookieConfig, CookieSession};
use tardis::web::poem::{
    self, handler,
    session::Session,
    web::{headers::HeaderMapExt, Form},
    Endpoint, Middleware,
};
use tardis::web::web_server::BoxMiddleware;
use tardis::TardisFuns;

use crate::config::SpacegateAdminConfig;
use crate::constants::DOMAIN_CODE;

pub(crate) mod auth_api;
pub(crate) mod backend_api;
pub(crate) mod dashboard_api;
pub(crate) mod gateway_api;
pub(crate) mod plugin_api;
pub(crate) mod route_api;
pub(crate) mod spacegate_manage_api;
pub(crate) mod tls_api;

async fn get_client_name(session: &Session) -> String {
    session.get::<String>("client_name").unwrap_or(DEFAULT_CLIENT_NAME.to_string()).clone()
}

#[derive(Default, Debug, Clone, Serialize, Deserialize)]
pub struct BasicAuth;

impl BasicAuth {
    pub fn boxed() -> BoxMiddleware<'static> {
        Box::new(BasicAuth)
    }
}

impl Middleware<BoxEndpoint<'static>> for BasicAuth {
    type Output = BoxEndpoint<'static>;

    fn transform(&self, ep: BoxEndpoint<'static>) -> Self::Output {
        Box::new(BasicAuthEndpoint(ep))
    }
}

pub struct BasicAuthEndpoint<E>(E);

#[poem::async_trait]
impl<E: Endpoint> Endpoint for BasicAuthEndpoint<E> {
    type Output = E::Output;

    async fn call(&self, req: poem::Request) -> poem::Result<Self::Output> {
        let config = TardisFuns::cs_config::<SpacegateAdminConfig>(DOMAIN_CODE);
        if let Some(basic_auth) = config.basic_auth.clone() {
            if let Some(auth) = req.headers().typed_get::<poem::web::headers::Authorization<poem::web::headers::authorization::Basic>>() {
                if auth.0.username() == basic_auth.username && auth.0.password() == basic_auth.password {
                    return self.0.call(req).await;
                }
            }
            Err(poem::Error::from_status(StatusCode::UNAUTHORIZED))
        } else {
            self.0.call(req).await
        }
    }
}

#[derive(Default, Debug, Clone, Serialize, Deserialize)]
pub struct CookieMW;

impl CookieMW {
    pub fn boxed() -> BoxMiddleware<'static> {
        Box::new(CookieMW)
    }
}

impl Middleware<BoxEndpoint<'static>> for CookieMW {
    type Output = BoxEndpoint<'static>;

    fn transform(&self, ep: BoxEndpoint<'static>) -> Self::Output {
        let config = TardisFuns::cs_config::<SpacegateAdminConfig>(DOMAIN_CODE);
        Box::new(CookieSession::new(CookieConfig::new().name(&config.cookie_config.name).path("/").http_only(true)).transform(ep))
    }
}
