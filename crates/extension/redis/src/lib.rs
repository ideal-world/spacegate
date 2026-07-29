use std::{collections::HashMap, sync::RwLock, time::Duration};

pub use deadpool_redis::{Connection, Manager, Pool, PoolError, TimeoutType};
pub use redis;
use redis::RedisResult;

/// Redis 连接池的容量和等待上限。
///
/// 显式配置避免在低 CPU 的 Pod 中落入 deadpool 的 CPU 相关小池默认值，
/// 并确保 Redis 异常不会无限挂起网关请求。
#[derive(Debug, Clone, Copy)]
pub struct RedisPoolConfig {
    /// 允许同时借出的最大 Redis 连接数。
    pub max_size: usize,
    /// 等待空闲连接的最大时间。
    pub wait_timeout: Duration,
    /// 新建 Redis 连接的最大时间。
    pub create_timeout: Duration,
    /// 校验或回收 Redis 连接的最大时间。
    pub recycle_timeout: Duration,
}

impl Default for RedisPoolConfig {
    fn default() -> Self {
        Self {
            max_size: 32,
            wait_timeout: Duration::from_secs(1),
            create_timeout: Duration::from_secs(1),
            recycle_timeout: Duration::from_secs(1),
        }
    }
}

/// Wrapper for pooled Redis client.
#[derive(Clone)]
pub struct RedisClient {
    /// Pooled Redis client.
    pub redis_conn_pool: Pool,
}

impl std::fmt::Debug for RedisClient {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("RedisClient").finish()
    }
}

impl RedisClient {
    /// Create a new Redis client from connect url.
    pub fn new(url: impl AsRef<str>) -> RedisResult<Self> {
        Self::new_with_pool_config(url, RedisPoolConfig::default())
    }

    /// Create a Redis client with explicit pool capacity and finite timeouts.
    pub fn new_with_pool_config(url: impl AsRef<str>, config: RedisPoolConfig) -> RedisResult<Self> {
        let url = url.as_ref();
        let mut pool_config = deadpool_redis::PoolConfig::new(config.max_size.max(1));
        pool_config.timeouts = deadpool_redis::Timeouts {
            wait: Some(config.wait_timeout),
            create: Some(config.create_timeout),
            recycle: Some(config.recycle_timeout),
        };
        let redis_conn_pool = Pool::builder(Manager::new(url)?).config(pool_config).runtime(deadpool_redis::Runtime::Tokio1).build().expect("Failed to create Redis pool");
        Ok(Self { redis_conn_pool })
    }
    /// Get a connection from the pool.
    pub async fn get_conn(&self) -> Result<Connection, PoolError> {
        self.redis_conn_pool.get().await
    }
}

impl From<&str> for RedisClient {
    fn from(url: &str) -> Self {
        Self::new(url).expect("Failed to create Redis client")
    }
}

/// Redis Client Repository.
#[derive(Debug, Default)]
pub struct RedisClientRepo {
    repos: RwLock<HashMap<String, RedisClient>>,
}

impl RedisClientRepo {
    /// Get the global Redis client repository instance.
    pub fn global() -> &'static Self {
        static INIT: std::sync::OnceLock<RedisClientRepo> = std::sync::OnceLock::new();
        INIT.get_or_init(Self::new)
    }

    /// Create a new Redis client repository.
    pub fn new() -> Self {
        Self { repos: RwLock::default() }
    }

    /// Add a Redis client to the repository.
    pub fn add(&self, name: impl Into<String>, client: impl Into<RedisClient>) {
        self.repos.write().expect("poisoned global redis client repo").insert(name.into(), client.into());
    }

    /// Get a Redis client from the repository by its name.
    pub fn get(&self, name: &str) -> Option<RedisClient> {
        self.repos.read().expect("poisoned global redis client repo").get(name).cloned()
    }

    /// Remove a Redis client from the repository by its name.
    pub fn remove(&self, name: &str) -> Option<RedisClient> {
        self.repos.write().expect("poisoned global redis client repo").remove(name)
    }
}

pub struct RedisClientRepoError {
    name: String,
    message: String,
}

impl RedisClientRepoError {
    pub fn new(name: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            message: message.into(),
        }
    }
}

impl std::fmt::Debug for RedisClientRepoError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("RedisClientRepoError").field("name", &self.name).field("message", &self.message).finish()
    }
}

impl std::fmt::Display for RedisClientRepoError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "RedisClientRepoError: name: {}, message: {}", self.name, self.message)
    }
}

impl std::error::Error for RedisClientRepoError {}

pub fn global_repo() -> &'static RedisClientRepo {
    RedisClientRepo::global()
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use super::*;
    use testcontainers::{clients::Cli, RunnableImage};
    use testcontainers_modules::redis::Redis;

    /// 等待已被占满的 Redis pool 必须在配置期限内返回错误，不能无限挂起网关请求。
    #[tokio::test]
    async fn get_conn_times_out_when_pool_capacity_is_exhausted() {
        let docker = Cli::default();
        let container = docker.run(RunnableImage::from(Redis));
        let port = container.get_host_port_ipv4(6379);
        let client = RedisClient::new_with_pool_config(
            format!("redis://127.0.0.1:{port}/0"),
            RedisPoolConfig {
                max_size: 1,
                wait_timeout: Duration::from_millis(50),
                create_timeout: Duration::from_secs(1),
                recycle_timeout: Duration::from_secs(1),
            },
        )
        .expect("Redis client");
        let _held = client.get_conn().await.expect("first connection");

        let error = match client.get_conn().await {
            Ok(_) => panic!("second checkout must time out"),
            Err(error) => error,
        };

        assert!(matches!(error, PoolError::Timeout(TimeoutType::Wait)), "error must identify pool wait timeout: {error}");
    }
}

pub trait AsRedisKey {
    fn as_redis_key(&self, prefix: impl AsRef<str>) -> String;
}
