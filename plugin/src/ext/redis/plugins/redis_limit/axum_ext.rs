use serde::{Deserialize, Serialize};

// pub fn create_route() -> axum::Router {
//     axum::Router::new().route("/", get(get_limit_rule))
// }

#[derive(Debug, Serialize, Deserialize)]
pub struct RateLimit {
    pub freq_limit: u32,
}

// async fn get_limit_rule(Query(match_specifier): Query<MatchSpecifier>, Query(gateway): Query<String>) -> Result<Json<Option<MatchSpecifier>>, InternalError> {
//     let client = global_repo().get(&gateway).ok_or_else(|| "gateway missing");
// }

// post
// async fn add_limit_rule(Query(match_specifier): Query<MatchSpecifier>, Json(rate_limit): Json<RateLimit>) -> Result<Json<MatchSpecifier>, BoxError> {
//     todo!()
// }

// /// put
// async fn update_limit_rule(Query(match_specifier): Query<MatchSpecifier>, Json(rate_limit): Json<RateLimit>) -> Result<Json<MatchSpecifier>, BoxError> {
//     todo!()
// }

// /// get
// async fn delete_limit_rule(Query(match_specifier): Query<MatchSpecifier>) -> Result<Json<Option<MatchSpecifier>>, BoxError> {
//     todo!()
// }
