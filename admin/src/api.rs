use tardis::web::poem::handler;

pub(crate) mod backend_api;
pub(crate) mod gateway_api;
pub(crate) mod plugin_api;
pub(crate) mod route_api;
pub(crate) mod spacegate_manage_api;
pub(crate) mod tls_api;

#[handler]
pub(crate) async fn get_client_name(session: &Session) -> String {
    let count = session.get::<String>("client_name").unwrap_or(DEFAULT_CLIENT_NAME);
}

#[handler]
pub(crate) async fn set_client_name(name: &str, session: &Session) {
    session.set("client_name", name);
}
