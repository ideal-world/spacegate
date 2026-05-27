//! 可在 host 侧单测的策略/JSON 纯逻辑（TC-GW / TC-HDR 相关）。

/// 规范化 X-RateLimit-Policy 取值。
pub fn normalize_policy(value: &str) -> Option<String> {
    match value.trim().to_ascii_lowercase().as_str() {
        "abandon" => Some("abandon".to_string()),
        "queue" => Some("queue".to_string()),
        "wait" => Some("wait".to_string()),
        _ => None,
    }
}

/// 规范化队列优先级 header。
pub fn normalize_priority(value: &str) -> Option<String> {
    match value.trim().to_ascii_lowercase().as_str() {
        "high" => Some("high".to_string()),
        "normal" | "default" | "medium" => Some("normal".to_string()),
        "low" => Some("low".to_string()),
        _ => None,
    }
}

/// 解析限流 check 响应中的 allowed=true。
pub fn contains_allowed_true(text: &str) -> bool {
    text.contains(r#""allowed":true"#) || text.contains(r#""allowed": true"#)
}

/// 从 JSON 文本提取数字字段（如 retry_after_ms）。
pub fn extract_json_number(text: &str, key: &str) -> Option<u64> {
    let quoted = format!("\"{key}\"");
    for needle in [format!("{quoted}:"), format!("{quoted}:\"")] {
        let Some(pos) = text.find(&needle) else { continue };
        let digits = text[pos + needle.len()..]
            .chars()
            .skip_while(|c| c.is_whitespace() || *c == '"')
            .take_while(|c| c.is_ascii_digit())
            .collect::<String>();
        if let Ok(v) = digits.parse() {
            return Some(v);
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tc_hdr_02_rejects_invalid_policy() {
        assert!(normalize_policy("invalid").is_none());
        assert_eq!(normalize_policy("QUEUE").as_deref(), Some("queue"));
    }

    #[test]
    fn tc_gw_rate_limit_response_parsing() {
        assert!(contains_allowed_true(r#"{"allowed":true,"retry_after_ms":0}"#));
        assert!(!contains_allowed_true(r#"{"allowed":false}"#));
        assert_eq!(extract_json_number(r#"{"retry_after_ms":3000}"#, "retry_after_ms"), Some(3000));
    }

    #[test]
    fn normalize_priority_values() {
        assert_eq!(normalize_priority("HIGH").as_deref(), Some("high"));
        assert_eq!(normalize_priority("medium").as_deref(), Some("normal"));
        assert!(normalize_priority("urgent").is_none());
    }
}
