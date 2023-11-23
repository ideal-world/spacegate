use k8s_openapi::http::StatusCode;
use kernel_common::client::k8s_client::DEFAULT_CLIENT_NAME;
use serde::{Deserialize, Serialize};
use tardis::web::poem::{self, handler, web::headers::HeaderMapExt, Endpoint, Middleware};

pub(crate) mod auth_api;
pub(crate) mod backend_api;
pub(crate) mod dashboard_api;
pub(crate) mod gateway_api;
pub(crate) mod plugin_api;
pub(crate) mod route_api;
pub(crate) mod spacegate_manage_api;
pub(crate) mod tls_api;

//todo session
// #[handler]
// async fn get_client_name(session: &Session) -> String {
//     session.get::<String>("client_name").unwrap_or(DEFAULT_CLIENT_NAME)
// }

// #[handler]
// pub(crate) async fn set_client_name(name: &str, session: &Session) {
//     session.set("client_name", name);
// }

#[derive(Default, Debug, Serialize, Deserialize)]
pub(crate) struct BasicAuth {
    username: String,
    password: String,
}

impl<E: Endpoint> Middleware<E> for BasicAuth {
    type Output = BasicAuthEndpoint<E>;

    fn transform(&self, ep: E) -> Self::Output {
        BasicAuthEndpoint {
            ep,
            username: self.username.clone(),
            password: self.password.clone(),
        }
    }
}

struct BasicAuthEndpoint<E> {
    ep: E,
    username: String,
    password: String,
}

#[poem::async_trait]
impl<E: Endpoint> Endpoint for BasicAuthEndpoint<E> {
    type Output = E::Output;

    async fn call(&self, req: poem::Request) -> poem::Result<Self::Output> {
        if let Some(auth) = req.headers().typed_get::<poem::web::headers::Authorization<poem::web::headers::authorization::Basic>>() {
            if auth.0.username() == self.username && auth.0.password() == self.password {
                return self.ep.call(req).await;
            }
        }
        Err(poem::Error::from_status(StatusCode::UNAUTHORIZED))
    }
}
