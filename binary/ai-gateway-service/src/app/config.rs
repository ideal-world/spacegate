use std::path::{Path as ConfigPath, PathBuf};

use clap::{parser::ValueSource, ArgMatches, CommandFactory, FromArgMatches};

/// 默认可执行文件同目录下的配置文件名。
const DEFAULT_CONFIG_FILE_NAME: &str = "ai-gateway-service.toml";

/// CLI 包装层：配置文件路径 + 原有 Args。
#[derive(Debug, Parser)]
#[command(version, about = "External Redis-backed rate-limit and queue service for SpaceGate AI gateway")]
struct Cli {
    /// TOML 配置文件路径；未指定时尝试读取可执行文件同目录下的 ai-gateway-service.toml。
    #[arg(long, env = "AI_GATEWAY_CONFIG", value_name = "FILE")]
    config: Option<PathBuf>,
    #[command(flatten)]
    args: Args,
}

/// 解析最终使用的配置文件路径：显式参数 > 可执行文件同目录默认文件。
fn resolve_config_path(explicit: Option<PathBuf>) -> Option<PathBuf> {
    if let Some(path) = explicit {
        return Some(path);
    }
    default_config_path_beside_executable()
}

/// 可执行文件所在目录下的默认配置文件（存在才返回）。
fn default_config_path_beside_executable() -> Option<PathBuf> {
    std::env::current_exe().ok().and_then(|exe| default_config_path_in_dir(&exe))
}

/// 给定可执行文件路径，返回同目录下默认配置文件路径（存在才返回）。
fn default_config_path_in_dir(exe_path: &ConfigPath) -> Option<PathBuf> {
    let dir = exe_path.parent()?;
    let path = dir.join(DEFAULT_CONFIG_FILE_NAME);
    path.is_file().then_some(path)
}

/// 从 CLI、环境变量和可选 TOML 配置文件合并出最终运行参数。
fn load_args() -> Result<Args, Box<dyn std::error::Error>> {
    let matches = Cli::command().get_matches();
    let explicit_config = matches.get_one::<PathBuf>("config").cloned();
    let config_path = resolve_config_path(explicit_config);
    let cli = Cli::from_arg_matches(&matches).expect("cli args");

    let file_args = match config_path.as_deref() {
        Some(path) => {
            tracing::info!(path = %path.display(), "loading config file");
            Some(ServiceConfigFile::load(path)?.into_args())
        }
        None => None,
    };

    Ok(merge_args(file_args, cli.args, &matches))
}

/// TOML 配置文件根结构；各 section 均可选，便于按需扩展。
#[derive(Debug, Default, Deserialize)]
#[serde(default)]
struct ServiceConfigFile {
    server: ServerSection,
    redis: RedisSection,
    upstream: UpstreamSection,
    queue: QueueSection,
    rate_limit: RateLimitSection,
    worker: WorkerSection,
    callback: CallbackSection,
    result: ResultSection,
    body: BodySection,
    object_store: ObjectStoreSection,
    admin: AdminSection,
}

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
struct ServerSection {
    host: Option<String>,
    port: Option<u16>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
struct RedisSection {
    /// Redis 连接 URL，例如 redis://127.0.0.1/ 或 redis://:password@host:6379/0
    url: Option<String>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
struct UpstreamSection {
    /// 上游 AI 服务地址；未配置时只入队，不启动 worker。
    base_url: Option<String>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
struct QueueSection {
    stream: Option<String>,
    high_stream: Option<String>,
    low_stream: Option<String>,
    enable_priority_streams: Option<bool>,
    default_priority: Option<String>,
    high_models: Option<Vec<String>>,
    low_models: Option<Vec<String>>,
    high_tenants: Option<Vec<String>>,
    low_tenants: Option<Vec<String>>,
    high_weight: Option<usize>,
    normal_weight: Option<usize>,
    low_weight: Option<usize>,
    max_len: Option<u64>,
    group: Option<String>,
    consumer: Option<String>,
    job_dlq_stream: Option<String>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
struct RateLimitSection {
    rps: Option<u64>,
    burst: Option<u64>,
    cost: Option<u64>,
    tenant_prefix: Option<String>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
struct WorkerSection {
    concurrency: Option<usize>,
    wait_timeout_secs: Option<u64>,
    reclaim_interval_secs: Option<u64>,
    reclaim_min_idle_secs: Option<u64>,
    job_process_lease_secs: Option<u64>,
    job_max_delivery_attempts: Option<u32>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
struct CallbackSection {
    require_https: Option<bool>,
    max_retry_attempts: Option<u32>,
    retry_initial_delay_ms: Option<u64>,
    retry_max_delay_ms: Option<u64>,
    retry_reclaim_idle_secs: Option<u64>,
    retry_stream: Option<String>,
    retry_group: Option<String>,
    dlq_stream: Option<String>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
struct ResultSection {
    key_prefix: Option<String>,
    channel_prefix: Option<String>,
    ttl_secs: Option<u64>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
struct BodySection {
    max_bytes: Option<usize>,
    inline_threshold: Option<usize>,
    read_concurrency: Option<usize>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
struct ObjectStoreSection {
    endpoint: Option<String>,
    bucket: Option<String>,
    prefix: Option<String>,
    multipart_part_size: Option<usize>,
    auth_header: Option<String>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
struct AdminSection {
    cors_origins: Option<Vec<String>>,
}

impl ServiceConfigFile {
    fn load(path: &ConfigPath) -> Result<Self, Box<dyn std::error::Error>> {
        let raw = std::fs::read_to_string(path).map_err(|e| format!("read config `{}`: {e}", path.display()))?;
        let cfg: Self = toml::from_str(&raw).map_err(|e| format!("parse config `{}`: {e}", path.display()))?;
        Ok(cfg)
    }

    fn into_args(self) -> Args {
        let mut args = Args::default();
        if let Some(host) = self.server.host {
            args.host = host.parse().unwrap_or(args.host);
        }
        if let Some(port) = self.server.port {
            args.port = port;
        }
        if let Some(url) = self.redis.url {
            args.redis_url = url;
        }
        args.upstream_base_url = self.upstream.base_url;

        if let Some(stream) = self.queue.stream {
            args.stream_key = stream;
        }
        if let Some(stream) = self.queue.high_stream {
            args.high_priority_stream_key = stream;
        }
        if let Some(stream) = self.queue.low_stream {
            args.low_priority_stream_key = stream;
        }
        if let Some(value) = self.queue.enable_priority_streams {
            args.enable_priority_streams = value;
        }
        if let Some(value) = self.queue.default_priority {
            args.queue_default_priority = value;
        }
        if let Some(values) = self.queue.high_models {
            args.queue_high_models = join_csv(&values);
        }
        if let Some(values) = self.queue.low_models {
            args.queue_low_models = join_csv(&values);
        }
        if let Some(values) = self.queue.high_tenants {
            args.queue_high_tenants = join_csv(&values);
        }
        if let Some(values) = self.queue.low_tenants {
            args.queue_low_tenants = join_csv(&values);
        }
        if let Some(value) = self.queue.high_weight {
            args.queue_high_weight = value;
        }
        if let Some(value) = self.queue.normal_weight {
            args.queue_normal_weight = value;
        }
        if let Some(value) = self.queue.low_weight {
            args.queue_low_weight = value;
        }
        if let Some(value) = self.queue.max_len {
            args.stream_max_len = value;
        }
        if let Some(value) = self.queue.group {
            args.consumer_group = value;
        }
        if let Some(value) = self.queue.consumer {
            args.consumer_name = value;
        }
        if let Some(value) = self.queue.job_dlq_stream {
            args.job_dlq_stream = value;
        }

        if let Some(value) = self.rate_limit.rps {
            args.rate_limit_rps = value;
        }
        if let Some(value) = self.rate_limit.burst {
            args.rate_limit_burst = value;
        }
        if let Some(value) = self.rate_limit.cost {
            args.rate_limit_cost = value;
        }
        if let Some(value) = self.rate_limit.tenant_prefix {
            args.tenant_rate_limit_prefix = value;
        }

        if let Some(value) = self.worker.concurrency {
            args.worker_concurrency = value;
        }
        if let Some(value) = self.worker.wait_timeout_secs {
            args.wait_timeout_secs = value;
        }
        if let Some(value) = self.worker.reclaim_interval_secs {
            args.reclaim_interval_secs = value;
        }
        if let Some(value) = self.worker.reclaim_min_idle_secs {
            args.reclaim_min_idle_secs = value;
        }
        if let Some(value) = self.worker.job_process_lease_secs {
            args.job_process_lease_secs = value;
        }
        if let Some(value) = self.worker.job_max_delivery_attempts {
            args.job_max_delivery_attempts = value;
        }

        if let Some(value) = self.callback.require_https {
            args.require_https_callback = value;
        }
        if let Some(value) = self.callback.max_retry_attempts {
            args.callback_max_retry_attempts = value;
        }
        if let Some(value) = self.callback.retry_initial_delay_ms {
            args.callback_retry_initial_delay_ms = value;
        }
        if let Some(value) = self.callback.retry_max_delay_ms {
            args.callback_retry_max_delay_ms = value;
        }
        if let Some(value) = self.callback.retry_reclaim_idle_secs {
            args.callback_retry_reclaim_idle_secs = value;
        }
        if let Some(value) = self.callback.retry_stream {
            args.callback_retry_stream = value;
        }
        if let Some(value) = self.callback.retry_group {
            args.callback_retry_group = value;
        }
        if let Some(value) = self.callback.dlq_stream {
            args.callback_dlq_stream = value;
        }

        if let Some(value) = self.result.key_prefix {
            args.result_key_prefix = value;
        }
        if let Some(value) = self.result.channel_prefix {
            args.result_channel_prefix = value;
        }
        if let Some(value) = self.result.ttl_secs {
            args.result_ttl_secs = value;
        }

        if let Some(value) = self.body.max_bytes {
            args.max_body_bytes = value;
        }
        if let Some(value) = self.body.inline_threshold {
            args.inline_threshold = value;
        }
        if let Some(value) = self.body.read_concurrency {
            args.body_read_concurrency = value;
        }

        args.object_store_endpoint = self.object_store.endpoint;
        if let Some(value) = self.object_store.bucket {
            args.object_store_bucket = value;
        }
        if let Some(value) = self.object_store.prefix {
            args.object_store_prefix = value;
        }
        if let Some(value) = self.object_store.multipart_part_size {
            args.object_multipart_part_size = value;
        }
        args.object_store_auth_header = self.object_store.auth_header;

        if let Some(values) = self.admin.cors_origins {
            args.admin_cors_origins = join_csv(&values);
        }

        args
    }
}

/// 合并优先级：显式 CLI / 环境变量 > 配置文件 > 内置默认值。
fn merge_args(file_args: Option<Args>, cli_args: Args, matches: &ArgMatches) -> Args {
    let file = file_args.unwrap_or_else(Args::default);
    let mut out = file;

    macro_rules! pick {
        ($field:ident, $id:expr) => {
            if is_explicit(matches, $id) {
                out.$field = cli_args.$field;
            }
        };
        ($field:ident, $id:expr, clone) => {
            if is_explicit(matches, $id) {
                out.$field = cli_args.$field.clone();
            }
        };
    }

    pick!(host, "host");
    pick!(port, "port");
    pick!(redis_url, "redis_url", clone);
    pick!(stream_key, "stream_key", clone);
    pick!(high_priority_stream_key, "high_priority_stream_key", clone);
    pick!(low_priority_stream_key, "low_priority_stream_key", clone);
    pick!(enable_priority_streams, "enable_priority_streams");
    pick!(queue_default_priority, "queue_default_priority", clone);
    pick!(queue_high_models, "queue_high_models", clone);
    pick!(queue_low_models, "queue_low_models", clone);
    pick!(queue_high_tenants, "queue_high_tenants", clone);
    pick!(queue_low_tenants, "queue_low_tenants", clone);
    pick!(queue_high_weight, "queue_high_weight");
    pick!(queue_normal_weight, "queue_normal_weight");
    pick!(queue_low_weight, "queue_low_weight");
    pick!(stream_max_len, "stream_max_len");
    pick!(consumer_group, "consumer_group", clone);
    pick!(consumer_name, "consumer_name", clone);
    pick!(job_dlq_stream, "job_dlq_stream", clone);
    pick!(callback_retry_stream, "callback_retry_stream", clone);
    pick!(callback_retry_group, "callback_retry_group", clone);
    pick!(callback_dlq_stream, "callback_dlq_stream", clone);
    pick!(callback_max_retry_attempts, "callback_max_retry_attempts");
    pick!(callback_retry_initial_delay_ms, "callback_retry_initial_delay_ms");
    pick!(callback_retry_max_delay_ms, "callback_retry_max_delay_ms");
    pick!(callback_retry_reclaim_idle_secs, "callback_retry_reclaim_idle_secs");
    pick!(result_key_prefix, "result_key_prefix", clone);
    pick!(result_channel_prefix, "result_channel_prefix", clone);
    pick!(result_ttl_secs, "result_ttl_secs");
    pick!(rate_limit_rps, "rate_limit_rps");
    pick!(rate_limit_burst, "rate_limit_burst");
    pick!(rate_limit_cost, "rate_limit_cost");
    pick!(tenant_rate_limit_prefix, "tenant_rate_limit_prefix", clone);
    pick!(wait_timeout_secs, "wait_timeout_secs");
    pick!(worker_concurrency, "worker_concurrency");
    pick!(admin_cors_origins, "admin_cors_origins", clone);
    pick!(max_body_bytes, "max_body_bytes");
    pick!(inline_threshold, "inline_threshold");
    pick!(body_read_concurrency, "body_read_concurrency");
    pick!(reclaim_interval_secs, "reclaim_interval_secs");
    pick!(reclaim_min_idle_secs, "reclaim_min_idle_secs");
    pick!(job_process_lease_secs, "job_process_lease_secs");
    pick!(job_max_delivery_attempts, "job_max_delivery_attempts");
    pick!(require_https_callback, "require_https_callback");
    pick!(object_store_bucket, "object_store_bucket", clone);
    pick!(object_store_prefix, "object_store_prefix", clone);
    pick!(object_multipart_part_size, "object_multipart_part_size");

    if is_explicit(matches, "upstream_base_url") {
        out.upstream_base_url = cli_args.upstream_base_url.clone();
    }
    if is_explicit(matches, "object_store_endpoint") {
        out.object_store_endpoint = cli_args.object_store_endpoint.clone();
    }
    if is_explicit(matches, "object_store_auth_header") {
        out.object_store_auth_header = cli_args.object_store_auth_header.clone();
    }

    out
}

fn is_explicit(matches: &ArgMatches, id: &str) -> bool {
    matches
        .value_source(id)
        .is_some_and(|source| matches!(source, ValueSource::CommandLine | ValueSource::EnvVariable))
}

fn join_csv(values: &[String]) -> String {
    values.iter().map(String::as_str).collect::<Vec<_>>().join(",")
}

#[cfg(test)]
mod config_tests {
    use super::*;
    use std::io::Write;

    #[test]
    fn loads_redis_and_upstream_from_toml() {
        let mut file = tempfile::NamedTempFile::new().expect("temp file");
        write!(
            file,
            r#"
[redis]
url = "redis://redis.example:6379/0"

[upstream]
base_url = "http://upstream.example:9000"

[server]
port = 19080
"#
        )
        .expect("write temp config");

        let cfg = ServiceConfigFile::load(file.path()).expect("load config");
        let args = cfg.into_args();
        assert_eq!(args.redis_url, "redis://redis.example:6379/0");
        assert_eq!(args.upstream_base_url.as_deref(), Some("http://upstream.example:9000"));
        assert_eq!(args.port, 19080);
    }

    #[test]
    fn resolve_config_path_prefers_explicit() {
        let explicit = PathBuf::from("/tmp/custom.toml");
        assert_eq!(resolve_config_path(Some(explicit.clone())), Some(explicit));
    }

    #[test]
    fn default_config_path_in_dir_finds_sibling_file() {
        let dir = tempfile::tempdir().expect("temp dir");
        let config = dir.path().join(DEFAULT_CONFIG_FILE_NAME);
        std::fs::write(&config, "[redis]\nurl = \"redis://127.0.0.1/\"").expect("write config");

        let fake_exe = dir.path().join("ai-gateway-service");
        std::fs::write(&fake_exe, b"").expect("write fake exe");

        assert_eq!(default_config_path_in_dir(&fake_exe), Some(config));
    }

    #[test]
    fn default_config_path_in_dir_returns_none_when_missing() {
        let dir = tempfile::tempdir().expect("temp dir");
        let fake_exe = dir.path().join("ai-gateway-service");
        std::fs::write(&fake_exe, b"").expect("write fake exe");

        assert_eq!(default_config_path_in_dir(&fake_exe), None);
    }
}
