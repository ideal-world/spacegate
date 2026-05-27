use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::Instant;

use chrono::{DateTime, Duration, Utc};
use hyper::Uri;
use serde::{Deserialize, Deserializer, Serialize};

use super::usage::TokenUsage;

const API_KEY_EMPTY_EXPIRED_AT_CACHE_SECS: i64 = 60;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApiKeyRecord {
    pub app_id: String,
    #[serde(default)]
    pub asset_ids: Vec<String>,
    #[serde(default)]
    pub allow_ips: Vec<String>,
    #[serde(default)]
    pub deny_ips: Vec<String>,
    #[serde(default)]
    pub allow_mac_addrs: Vec<String>,
    #[serde(default)]
    pub deny_mac_addrs: Vec<String>,
    #[serde(default = "default_api_key_expired_at", deserialize_with = "deserialize_api_key_expired_at")]
    pub expired_at: DateTime<Utc>,
}

fn default_api_key_expired_at() -> DateTime<Utc> {
    Utc::now() + Duration::seconds(API_KEY_EMPTY_EXPIRED_AT_CACHE_SECS)
}

fn deserialize_api_key_expired_at<'de, D>(deserializer: D) -> Result<DateTime<Utc>, D::Error>
where
    D: Deserializer<'de>,
{
    let value = Option::<String>::deserialize(deserializer)?;
    let Some(value) = value.map(|v| v.trim().to_string()) else {
        return Ok(default_api_key_expired_at());
    };
    if value.is_empty() {
        return Ok(default_api_key_expired_at());
    }
    value.parse::<DateTime<Utc>>().map_err(serde::de::Error::custom)
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AssetRecord {
    pub asset_id: String,
    pub asset_type: String,
    pub asset_status: String,
    #[serde(default)]
    pub runtime_endpoint: Option<String>,
    #[serde(default)]
    pub runtime_endpoint_method: Vec<String>,
    #[serde(default)]
    pub asset_content: Option<String>,
    #[serde(default)]
    pub asset_url: Option<String>,
    #[serde(default)]
    pub max_concurrent: Option<u32>,
    #[serde(default)]
    pub timeout_sec: Option<u64>,
    #[serde(default)]
    pub qps_limit: Option<u32>,
    #[serde(default)]
    pub asset_secret_params: Vec<SecretParam>,
    #[serde(default)]
    pub asset_secret_values: HashMap<String, String>,
    #[serde(default)]
    pub allowed_output_targets: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SecretTargetType {
    Header,
    Query,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SecretParam {
    pub secret_key: String,
    #[serde(default)]
    pub header_name: String,
    #[serde(default)]
    pub required: bool,
    #[serde(rename = "type", default)]
    pub target_type: Option<SecretTargetType>,
    #[serde(default)]
    pub name: Option<String>,
}

impl SecretParam {
    pub fn effective_target_type(&self) -> SecretTargetType {
        self.target_type.unwrap_or(SecretTargetType::Header)
    }

    pub fn effective_name(&self) -> Option<&str> {
        self.name.as_deref().map(str::trim).filter(|value| !value.is_empty()).or_else(|| Some(self.header_name.trim())).filter(|value| !value.is_empty())
    }
}

#[derive(Debug, Clone)]
pub struct HaiRequestContext {
    pub asset_id: String,
    pub asset_version: Option<String>,
    pub api_key_hash: String,
    pub client_ip: String,
    pub client_mac: String,
    pub request_id: String,
}

#[derive(Debug, Clone)]
pub struct HaiApiIdentity(pub ApiKeyRecord);

#[derive(Debug, Clone)]
pub struct HaiAsset(pub AssetRecord);

#[derive(Debug, Clone)]
pub struct HaiDispatch {
    pub protocol: String,
    pub upstream: Option<Uri>,
    pub timeout_ms: Option<u64>,
    pub is_model: bool,
    pub is_streaming: bool,
}

#[derive(Debug, Clone)]
pub struct HaiUsageState {
    pub usage: TokenUsage,
    pub status: Option<u16>,
    pub error_code: Option<String>,
    pub output_blocked: bool,
    pub output_block_reason: Option<String>,
    pub started_at: Instant,
}

impl Default for HaiUsageState {
    fn default() -> Self {
        Self {
            usage: TokenUsage::default(),
            status: None,
            error_code: None,
            output_blocked: false,
            output_block_reason: None,
            started_at: Instant::now(),
        }
    }
}

#[derive(Debug, Default, Clone)]
pub struct HaiAuditData {
    pub request: Option<HaiRequestContext>,
    pub identity: Option<ApiKeyRecord>,
    pub asset: Option<AssetRecord>,
    pub dispatch: Option<HaiDispatch>,
    pub usage: HaiUsageState,
}

#[derive(Debug, Clone, Default)]
pub struct HaiAuditState(pub Arc<Mutex<HaiAuditData>>);

impl HaiAuditState {
    pub fn snapshot(&self) -> HaiAuditData {
        self.0.lock().map(|data| data.clone()).unwrap_or_default()
    }

    pub fn update(&self, f: impl FnOnce(&mut HaiAuditData)) {
        if let Ok(mut data) = self.0.lock() {
            f(&mut data);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Duration;

    #[test]
    fn api_key_record_empty_expired_at_uses_short_fallback_expiry() {
        let before = Utc::now();
        let rec: ApiKeyRecord = serde_json::from_str(
            r#"{
                "app_id": "app-empty-expiry",
                "asset_ids": ["asset-1"],
                "expired_at": ""
            }"#,
        )
        .unwrap();
        let after = Utc::now();

        assert_eq!(rec.app_id, "app-empty-expiry");
        assert!(rec.expired_at >= before + Duration::seconds(55));
        assert!(rec.expired_at <= after + Duration::seconds(65));
    }

    #[test]
    fn secret_param_new_type_name_format_overrides_old_header_name() {
        let param: SecretParam = serde_json::from_str(
            r#"{
                "secret_key": "API_TOKEN",
                "header_name": "Authorization",
                "required": true,
                "type": "query",
                "name": "token"
            }"#,
        )
        .unwrap();

        assert_eq!(param.effective_target_type(), SecretTargetType::Query);
        assert_eq!(param.effective_name(), Some("token"));
    }
}
