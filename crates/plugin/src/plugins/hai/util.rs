use std::net::{IpAddr, Ipv4Addr, Ipv6Addr};

use hyper::{header, Request};
use serde::{Deserialize, Serialize};
use spacegate_ext_redis::{global_repo, RedisClient};
use spacegate_kernel::{extension::GatewayName, BoxError, SgBody};
use url::form_urlencoded;

use super::types::{AssetRecord, HaiAsset, HaiRequestContext, SecretTargetType};

pub const HAI_ASSET_VERSION_HEADER: &str = "hai-asset-version";
pub const API_KEY_PREFIX: &str = "hai:apikey:";
pub const ASSET_PREFIX: &str = "hai:asset:";
pub const QUOTA_QPS_PREFIX: &str = "hai:quota:qps:";
pub const QUOTA_CONCURRENT_PREFIX: &str = "hai:quota:concurrent:";

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(rename_all = "snake_case")]
pub enum MissingAssetPolicy {
    Error,
    Skip,
}

impl Default for MissingAssetPolicy {
    fn default() -> Self {
        Self::Error
    }
}

pub fn api_key_key(api_key: &str) -> String {
    format!("{API_KEY_PREFIX}{api_key}")
}

pub fn asset_key(asset_id: &str, version: Option<&str>) -> String {
    match normalize_optional_header_value(version) {
        Some(version) => format!("{ASSET_PREFIX}{asset_id}:version:{version}"),
        None => format!("{ASSET_PREFIX}{asset_id}"),
    }
}

pub fn quota_qps_key(asset_id: &str) -> String {
    format!("{QUOTA_QPS_PREFIX}{asset_id}")
}

pub fn quota_concurrent_key(asset_id: &str) -> String {
    format!("{QUOTA_CONCURRENT_PREFIX}{asset_id}")
}

pub fn parse_path(path: &str) -> Option<String> {
    let (raw, query) = split_path_and_query(path);
    let segments: Vec<&str> = raw.trim_start_matches('/').split('/').collect();

    match segments.as_slice() {
        ["api", "v1", _asset_type, asset_id, ..] if !asset_id.is_empty() => Some((*asset_id).to_string()),
        ["mcp-services", asset_id, ..] if !asset_id.is_empty() => Some((*asset_id).to_string()),
        ["api", "v1", _asset_type] => query.as_deref().and_then(asset_id_from_query),
        ["mcp-services"] => query.as_deref().and_then(asset_id_from_query),
        _ => None,
    }
}

pub fn parse_asset_type(path: &str) -> Option<String> {
    let (raw, _) = split_path_and_query(path);
    let segments: Vec<&str> = raw.trim_start_matches('/').split('/').collect();
    match segments.as_slice() {
        ["api", "v1", asset_type, _asset_id, ..] if !asset_type.is_empty() => Some((*asset_type).to_string()),
        ["api", "v1", asset_type] if !asset_type.is_empty() => Some((*asset_type).to_string()),
        _ => None,
    }
}

pub fn is_mcp_path(path: &str) -> bool {
    let (raw, _) = split_path_and_query(path);
    raw == "/mcp-services" || raw.starts_with("/mcp-services/")
}

pub fn asset_type_matches_path(path: &str, asset_type: &str) -> bool {
    if is_mcp_path(path) {
        return asset_type == "mcp";
    }
    parse_asset_type(path).map(|requested| requested == asset_type).unwrap_or(true)
}

pub fn split_path_and_query(path: &str) -> (String, Option<String>) {
    match path.split_once('?') {
        Some((path, query)) => (path.to_string(), Some(query.to_string())),
        None => (path.to_string(), None),
    }
}

fn asset_id_from_query(query: &str) -> Option<String> {
    form_urlencoded::parse(query.as_bytes()).find_map(|(k, v)| (k == "asset_id" && !v.is_empty()).then(|| v.into_owned()))
}

pub fn normalize_optional_header_value(value: Option<&str>) -> Option<String> {
    value.map(str::trim).filter(|v| !v.is_empty()).map(ToString::to_string)
}

pub fn extract_bearer_token(value: Option<&str>) -> Option<String> {
    value.and_then(|raw| {
        let trimmed = raw.trim();
        let mut parts = trimmed.splitn(2, char::is_whitespace);
        let scheme = parts.next().unwrap_or("");
        let token = parts.next().unwrap_or("").trim();
        if scheme.eq_ignore_ascii_case("bearer") && !token.is_empty() {
            Some(token.to_string())
        } else {
            None
        }
    })
}

pub fn hash_api_key(api_key: &str) -> String {
    let mut hash: u64 = 0xcbf29ce484222325;
    for byte in api_key.as_bytes() {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x100000001b3);
    }
    format!("fnv1a64:{hash:016x}")
}

pub fn first_forwarded_ip(xff: &str) -> Option<String> {
    xff.split(',').map(str::trim).find(|s| !s.is_empty()).map(String::from)
}

pub fn host_from_address(address: &str) -> String {
    let trimmed = address.trim();
    if trimmed.starts_with('[') {
        return trimmed.trim_start_matches('[').split(']').next().unwrap_or("").to_string();
    }
    trimmed.split(':').next().unwrap_or("").to_string()
}

pub fn resolve_client_ip(source_address: &str, x_forwarded_for: Option<&str>, x_real_ip: Option<&str>, trusted_proxy_cidrs: &[String]) -> String {
    let source_ip = host_from_address(source_address);
    let source_is_trusted = !source_ip.is_empty() && trusted_proxy_cidrs.iter().any(|rule| addr_matches_rule(&source_ip, rule));

    if source_is_trusted {
        if let Some(ip) = x_forwarded_for.and_then(first_forwarded_ip) {
            return ip;
        }
        if let Some(ip) = x_real_ip.map(str::trim).filter(|ip| !ip.is_empty()) {
            return ip.to_string();
        }
    }

    source_ip
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum IpNet {
    V4 { network: u32, prefix: u8 },
    V6 { network: u128, prefix: u8 },
}

impl IpNet {
    fn parse(raw: &str) -> Option<Self> {
        let (ip, prefix) = raw.trim().split_once('/')?;
        let prefix: u8 = prefix.parse().ok()?;

        if let Ok(ip) = ip.parse::<Ipv4Addr>() {
            if prefix > 32 {
                return None;
            }
            return Some(Self::V4 { network: u32::from(ip), prefix });
        }

        let ip = ip.parse::<Ipv6Addr>().ok()?;
        if prefix > 128 {
            return None;
        }
        Some(Self::V6 { network: u128::from(ip), prefix })
    }

    fn contains(&self, ip: &IpAddr) -> bool {
        match (self, ip) {
            (Self::V4 { network, prefix }, IpAddr::V4(ip)) => {
                let mask = if *prefix == 0 { 0 } else { u32::MAX << (32 - *prefix) };
                (u32::from(*ip) & mask) == (*network & mask)
            }
            (Self::V6 { network, prefix }, IpAddr::V6(ip)) => {
                let mask = if *prefix == 0 { 0 } else { u128::MAX << (128 - *prefix) };
                (u128::from(*ip) & mask) == (*network & mask)
            }
            _ => false,
        }
    }
}

fn addr_matches_rule(addr: &str, rule: &str) -> bool {
    let addr = addr.trim();
    let rule = rule.trim();
    if addr == rule {
        return true;
    }

    let Ok(ip) = addr.parse::<IpAddr>() else {
        return false;
    };
    IpNet::parse(rule).map(|net| net.contains(&ip)).unwrap_or(false)
}

pub fn check_addr(addr: &str, allow: &[String], deny: &[String]) -> bool {
    if deny.iter().any(|rule| addr_matches_rule(addr, rule)) {
        return false;
    }
    if !allow.is_empty() && !allow.iter().any(|rule| addr_matches_rule(addr, rule)) {
        return false;
    }
    true
}

pub fn redis_client(req: &Request<SgBody>) -> Result<RedisClient, BoxError> {
    let Some(gateway_name) = req.extensions().get::<GatewayName>() else {
        return Err("missing gateway name".into());
    };
    global_repo().get(gateway_name).ok_or_else(|| "missing redis client".into())
}

pub fn redis_client_from_url(redis_url: Option<&str>) -> Result<Option<RedisClient>, BoxError> {
    redis_url.map(RedisClient::new).transpose().map_err(Into::into)
}

pub fn redis_client_or_gateway(configured: Option<&RedisClient>, req: &Request<SgBody>) -> Result<RedisClient, BoxError> {
    configured.cloned().map_or_else(|| redis_client(req), Ok)
}

pub async fn load_asset_from_client(client: &RedisClient, asset_id: &str, version: Option<&str>) -> Result<Option<AssetRecord>, BoxError> {
    use spacegate_ext_redis::redis::AsyncCommands as _;

    let mut conn = client.get_conn().await;
    let raw: Option<String> = conn.get(asset_key(asset_id, version)).await?;
    raw.map(|raw| serde_json::from_str::<AssetRecord>(&raw).map_err(Into::into)).transpose()
}

pub async fn current_asset_with_client(req: &Request<SgBody>, configured: Option<&RedisClient>, allow_self_lookup: bool) -> Result<Option<AssetRecord>, BoxError> {
    if let Some(asset) = req.extensions().get::<HaiAsset>() {
        return Ok(Some(asset.0.clone()));
    }
    if !allow_self_lookup {
        return Ok(None);
    }
    let path = req.uri().path_and_query().map(|pq| pq.as_str()).unwrap_or_else(|| req.uri().path()).to_string();
    let Some(asset_id) = req.extensions().get::<HaiRequestContext>().map(|ctx| ctx.asset_id.clone()).or_else(|| parse_path(&path)) else {
        return Ok(None);
    };
    let version = req.extensions().get::<HaiRequestContext>().and_then(|ctx| ctx.asset_version.clone());
    let client = redis_client_or_gateway(configured, req)?;
    let Some(asset) = load_asset_from_client(&client, &asset_id, version.as_deref()).await? else {
        return Ok(None);
    };
    Ok(asset_type_matches_path(&path, &asset.asset_type).then_some(asset))
}

pub fn merge_query_params(endpoint: &str, overlay: &[(String, String)]) -> Result<String, BoxError> {
    let mut parsed = url::Url::parse(endpoint)?;
    let mut pairs = parsed.query().map(|query| form_urlencoded::parse(query.as_bytes()).map(|(k, v)| (k.into_owned(), v.into_owned())).collect::<Vec<_>>()).unwrap_or_default();
    for (key, value) in overlay {
        pairs.retain(|(existing_key, _)| existing_key != key);
        pairs.push((key.clone(), value.clone()));
    }
    if pairs.is_empty() {
        parsed.set_query(None);
    } else {
        let mut serializer = form_urlencoded::Serializer::new(String::new());
        for (key, value) in pairs {
            serializer.append_pair(&key, &value);
        }
        parsed.set_query(Some(&serializer.finish()));
    }
    Ok(parsed.to_string())
}

pub fn inject_asset_secrets(req: &mut Request<SgBody>, asset: &AssetRecord, endpoint: &str) -> Result<String, BoxError> {
    let mut query_overlay = Vec::new();
    for param in &asset.asset_secret_params {
        let Some(value) = asset.asset_secret_values.get(&param.secret_key) else {
            if param.required {
                return Err(format!("missing secret: {}", param.secret_key).into());
            }
            continue;
        };
        let Some(name) = param.effective_name() else {
            if param.required {
                return Err(format!("missing secret target name: {}", param.secret_key).into());
            }
            continue;
        };
        match param.effective_target_type() {
            SecretTargetType::Header => {
                req.headers_mut().insert(hyper::header::HeaderName::from_bytes(name.as_bytes())?, value.parse()?);
            }
            SecretTargetType::Query => query_overlay.push((name.to_string(), value.clone())),
        }
    }
    if query_overlay.is_empty() {
        Ok(endpoint.to_string())
    } else {
        merge_query_params(endpoint, &query_overlay)
    }
}

pub fn request_id(req: &Request<SgBody>) -> String {
    req.headers().get("x-request-id").and_then(|v| v.to_str().ok()).unwrap_or_default().to_string()
}

pub fn header_str<'a>(req: &'a Request<SgBody>, name: &str) -> Option<&'a str> {
    req.headers().get(name).and_then(|v| v.to_str().ok())
}

pub fn content_type_is_sse(headers: &hyper::HeaderMap) -> bool {
    headers.get(header::CONTENT_TYPE).and_then(|value| value.to_str().ok()).map(|value| value.to_ascii_lowercase().contains("text/event-stream")).unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builds_redis_keys_for_api_key_and_asset_version() {
        assert_eq!(api_key_key("sk-demo"), "hai:apikey:sk-demo");
        assert_eq!(asset_key("asset-1", None), "hai:asset:asset-1");
        assert_eq!(asset_key("asset-1", Some("v2")), "hai:asset:asset-1:version:v2");
    }

    #[test]
    fn api_key_hash_does_not_contain_plaintext() {
        let hash = hash_api_key("sk-secret-value");
        assert!(hash.starts_with("fnv1a64:"));
        assert!(!hash.contains("sk-secret-value"));
    }

    #[test]
    fn parse_path_supports_api_and_mcp_paths() {
        assert_eq!(parse_path("/api/v1/model/deepseek/chat"), Some("deepseek".to_string()));
        assert_eq!(parse_path("/mcp-services/tool-a/sse"), Some("tool-a".to_string()));
        assert_eq!(parse_path("/api/v1/model?asset_id=query-asset"), Some("query-asset".to_string()));
    }

    #[test]
    fn asset_type_matches_api_path_asset_type() {
        assert!(asset_type_matches_path("/api/v1/model/deepseek/chat", "model"));
        assert!(!asset_type_matches_path("/api/v1/model/deepseek/chat", "api"));
        assert!(asset_type_matches_path("/api/v1/mcp?asset_id=tool-a", "mcp"));
    }

    #[test]
    fn asset_type_matches_mcp_services_only_for_mcp_assets() {
        assert!(asset_type_matches_path("/mcp-services/tool-a/sse", "mcp"));
        assert!(!asset_type_matches_path("/mcp-services/tool-a/sse", "model"));
        assert!(!asset_type_matches_path("/mcp-services?asset_id=tool-a", "api"));
    }

    #[test]
    fn check_addr_honors_deny_before_allow() {
        assert!(!check_addr("10.1.1.1", &["10.0.0.0/8".to_string()], &["10.1.0.0/16".to_string()]));
        assert!(check_addr("10.2.1.1", &["10.0.0.0/8".to_string()], &["10.1.0.0/16".to_string()]));
    }
}
