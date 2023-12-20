use tardis::web::poem_openapi;
use tardis::web::web_resp::{TardisApiResult, TardisResp, Void};

#[derive(Clone, Default)]
pub struct AuthApi;

/// Auth API
#[poem_openapi::OpenApi(prefix_path = "/")]
impl AuthApi {
    #[oai(path = "/login", method = "post")]
    async fn login(&self) -> TardisApiResult<Void> {
        TardisResp::ok(Void {})
    }
}
