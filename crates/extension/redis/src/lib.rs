use std::{collections::HashMap, sync::RwLock};

pub use deadpool_redis::{Connection, Manager, Pool, PoolError};
pub use redis;
use redis::RedisResult;

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
    fn build_pool(url: &str) -> Result<Pool, PoolError> {
        let manager = Manager::new(url).map_err(PoolError::Backend)?;
        let redis_conn_pool =
            Pool::builder(manager).build().map_err(|e| PoolError::Backend(redis::RedisError::from((redis::ErrorKind::ClientError, "pool build failed", e.to_string()))))?;
        Ok(redis_conn_pool)
    }

    fn pool_error_into_redis_error(error: PoolError) -> redis::RedisError {
        match error {
            PoolError::Backend(error) => error,
            other => redis::RedisError::from((redis::ErrorKind::ClientError, "redis pool error", other.to_string())),
        }
    }

    /// Create a new Redis client from connect url.
    ///
    /// # Errors
    /// Returns an error if the url cannot be parsed or the pool builder fails.
    pub fn new(url: impl AsRef<str>) -> RedisResult<Self> {
        let redis_conn_pool = Self::build_pool(url.as_ref()).map_err(Self::pool_error_into_redis_error)?;
        Ok(Self { redis_conn_pool })
    }

    /// Create a new Redis client from connect url without panicking on pool setup failures.
    ///
    /// # Errors
    /// Returns an error if the url cannot be parsed or the pool builder fails.
    pub fn try_new(url: impl AsRef<str>) -> Result<Self, PoolError> {
        let redis_conn_pool = Self::build_pool(url.as_ref())?;
        Ok(Self { redis_conn_pool })
    }

    /// Get a connection from the pool.
    pub async fn get_conn(&self) -> Connection {
        self.try_get_conn().await.expect("failed to acquire redis connection")
    }

    /// Get a connection from the pool.
    ///
    /// # Errors
    /// Returns an error if the pool is closed, exhausted (timeout), or the
    /// underlying Redis connection fails.
    pub async fn try_get_conn(&self) -> Result<Connection, PoolError> {
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

pub trait AsRedisKey {
    fn as_redis_key(&self, prefix: impl AsRef<str>) -> String;
}
