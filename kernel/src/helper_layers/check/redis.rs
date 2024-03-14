use std::sync::Arc;

use futures_util::future::BoxFuture;
use hyper::Response;
use spacegate_ext_redis::{redis::Script, AsRedisKey, Connection, RedisClient};

use crate::{Marker, SgBody};

use super::Check;

/// check some extracted marker by using redis
#[derive(Clone)]
pub struct RedisCheck {
    pub check_script: Option<RedisCheckScript>,
    pub response_script: Option<RedisResponseScript>,
    pub key_prefix: Arc<str>,
    pub client: RedisClient,
}

#[derive(Clone)]
pub enum RedisCheckScript {
    Lua(Arc<Script>),
    Rust(Arc<dyn Fn(Connection, String) -> BoxFuture<'static, bool> + Send + Sync>),
}

impl std::fmt::Debug for RedisCheck {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("RedisCheck").field("key_prefix", &self.key_prefix).finish()
    }
}

impl From<Script> for RedisCheckScript {
    fn from(script: Script) -> Self {
        RedisCheckScript::Lua(Arc::new(script))
    }
}

impl From<Arc<Script>> for RedisCheckScript {
    fn from(script: Arc<Script>) -> Self {
        RedisCheckScript::Lua(script)
    }
}

impl From<Box<dyn Fn(Connection, String) -> BoxFuture<'static, bool> + Send + Sync>> for RedisCheckScript {
    fn from(f: Box<dyn Fn(Connection, String) -> BoxFuture<'static, bool> + Send + Sync>) -> Self {
        RedisCheckScript::Rust(Arc::new(f))
    }
}

#[derive(Clone)]
pub enum RedisResponseScript {
    Lua(Arc<Script>),
    Rust(Arc<dyn Fn(Connection, String, u16) -> BoxFuture<'static, ()> + Send + Sync>),
}

impl From<Script> for RedisResponseScript {
    fn from(script: Script) -> Self {
        RedisResponseScript::Lua(Arc::new(script))
    }
}

impl From<Arc<Script>> for RedisResponseScript {
    fn from(script: Arc<Script>) -> Self {
        RedisResponseScript::Lua(script)
    }
}

impl From<Box<dyn Fn(Connection, String, u16) -> BoxFuture<'static, ()> + Send + Sync>> for RedisResponseScript {
    fn from(f: Box<dyn Fn(Connection, String, u16) -> BoxFuture<'static, ()> + Send + Sync>) -> Self {
        RedisResponseScript::Rust(Arc::new(f))
    }
}

impl RedisCheckScript {
    pub async fn call(&self, mut conn: Connection, key: String) -> bool {
        match self {
            RedisCheckScript::Lua(script) => {
                let result: Result<bool, _> = script
                    // counter key
                    .key(&key)
                    .invoke_async(&mut conn)
                    .await;
                result
                    .inspect_err(|e| {
                        tracing::error!("Failed to run redis check script {}", e);
                    })
                    .unwrap_or(false)
            }
            RedisCheckScript::Rust(f) => f(conn, key).await,
        }
    }
}

impl RedisResponseScript {
    pub async fn call(&self, mut conn: Connection, key: String, status: u16) {
        match self {
            RedisResponseScript::Lua(script) => {
                let result: Result<(), _> = script
                    // counter key
                    .key(&key)
                    .arg(status)
                    .invoke_async(&mut conn)
                    .await;
                if let Err(e) = result {
                    tracing::error!("Failed to run redis response script {}", e);
                }
            }
            RedisResponseScript::Rust(f) => f(conn, key, status).await,
        }
    }
}

impl<M> Check<M> for RedisCheck
where
    M: AsRedisKey + Marker + Send + Sync + 'static,
{
    async fn check(&self, marker: &M) -> bool {
        let script = self.check_script.as_ref();
        let key = marker.as_redis_key(&self.key_prefix);
        if let Some(script) = script {
            script.call(self.client.get_conn().await, key).await
        } else {
            true
        }
    }
    fn on_response(&self, marker: M, resp: Response<SgBody>) -> Response<SgBody> {
        if let Some(script) = &self.response_script {
            if !resp.status().is_success() {
                let script = script.clone();
                let key = marker.as_redis_key(&self.key_prefix);
                let client = self.client.clone();
                let status = resp.status().as_u16();
                let task = async move {
                    let conn = client.get_conn().await;
                    script.call(conn, key, status).await;
                };
                tokio::spawn(task);
            }
        }
        resp
    }
}
