use std::future::Future;

use deadpool_redis::PoolError;
use spacegate_ext_redis::{redis::RedisResult, Connection, RedisClient, RedisClientRepo};

fn legacy_new(url: &str) -> RedisResult<RedisClient> {
    RedisClient::new(url)
}

fn legacy_get_conn(client: &RedisClient) -> impl Future<Output = Connection> + '_ {
    client.get_conn()
}

fn fallible_new(url: &str) -> Result<RedisClient, PoolError> {
    RedisClient::try_new(url)
}

fn fallible_get_conn(client: &RedisClient) -> impl Future<Output = Result<Connection, PoolError>> + '_ {
    client.try_get_conn()
}

#[test]
fn legacy_api_still_compiles() {
    let client = legacy_new("redis://127.0.0.1:6379").expect("failed to build legacy redis client");
    let repo = RedisClientRepo::new();

    repo.add("legacy-gateway", "redis://127.0.0.1:6379");
    let _client_from_str: RedisClient = "redis://127.0.0.1:6379".into();
    let _legacy_conn = legacy_get_conn(&client);
}

#[test]
fn fallible_api_still_compiles() {
    let client = fallible_new("redis://127.0.0.1:6379").expect("failed to build fallible redis client");
    let _fallible_conn = fallible_get_conn(&client);
}
