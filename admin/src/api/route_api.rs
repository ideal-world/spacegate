use tardis::web::poem_openapi;

#[derive(Clone, Default)]
pub struct HttprouteApi;

#[poem_openapi::OpenApi(prefix_path = "/httproute")]
impl HttprouteApi {}
