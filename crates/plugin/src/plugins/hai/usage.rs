use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct TokenUsage {
    pub prompt_tokens: Option<u64>,
    pub completion_tokens: Option<u64>,
    pub total_tokens: Option<u64>,
    pub cache_hit_tokens: Option<u64>,
    pub cache_miss_tokens: Option<u64>,
    pub tokens_per_second: Option<f64>,
    pub time_to_first_token_ms: Option<f64>,
    pub time_per_output_token_ms: Option<f64>,
}

impl TokenUsage {
    pub fn merge_from(&mut self, other: TokenUsage) {
        if other.prompt_tokens.is_some() {
            self.prompt_tokens = other.prompt_tokens;
        }
        if other.completion_tokens.is_some() {
            self.completion_tokens = other.completion_tokens;
        }
        if other.total_tokens.is_some() {
            self.total_tokens = other.total_tokens;
        }
        if other.cache_hit_tokens.is_some() {
            self.cache_hit_tokens = other.cache_hit_tokens;
        }
        if other.cache_miss_tokens.is_some() {
            self.cache_miss_tokens = other.cache_miss_tokens;
        }
        if other.tokens_per_second.is_some() {
            self.tokens_per_second = other.tokens_per_second;
        }
        if other.time_to_first_token_ms.is_some() {
            self.time_to_first_token_ms = other.time_to_first_token_ms;
        }
        if other.time_per_output_token_ms.is_some() {
            self.time_per_output_token_ms = other.time_per_output_token_ms;
        }
    }
}

pub fn parse_usage_from_json_bytes(raw: &[u8]) -> Option<TokenUsage> {
    let value: Value = serde_json::from_slice(raw).ok()?;
    let usage = value.get("usage")?;
    parse_usage_value(usage)
}

pub fn parse_usage_from_sse_chunk(raw: &[u8]) -> Option<TokenUsage> {
    let text = String::from_utf8_lossy(raw);
    let mut merged = TokenUsage::default();
    let mut found = false;
    let mut event_data = String::new();

    for line in text.lines() {
        let line = line.trim_start();
        let Some(data) = line.strip_prefix("data:") else {
            if line.is_empty() && !event_data.is_empty() {
                if merge_usage_from_sse_data(&event_data, &mut merged) {
                    found = true;
                }
                event_data.clear();
            } else if !event_data.is_empty() {
                event_data.push('\n');
                event_data.push_str(line);
            }
            continue;
        };
        let data = data.trim();
        if data.is_empty() {
            continue;
        }
        if !event_data.is_empty() {
            event_data.push('\n');
        }
        event_data.push_str(data);
    }

    if !event_data.is_empty() && merge_usage_from_sse_data(&event_data, &mut merged) {
        found = true;
    }

    found.then_some(merged)
}

fn merge_usage_from_sse_data(data: &str, merged: &mut TokenUsage) -> bool {
    let data = data.trim();
    if data.is_empty() || data == "[DONE]" {
        return false;
    }
    if let Some(usage) = parse_usage_from_json_bytes(data.as_bytes()) {
        merged.merge_from(usage);
        return true;
    }
    false
}

fn parse_usage_value(usage: &Value) -> Option<TokenUsage> {
    let prompt_tokens = usage.get("prompt_tokens").and_then(Value::as_u64);
    let completion_tokens = usage.get("completion_tokens").and_then(Value::as_u64);
    let total_tokens = usage.get("total_tokens").and_then(Value::as_u64);
    let nested_cached_tokens = usage.get("prompt_tokens_details").and_then(|details| details.get("cached_tokens")).and_then(Value::as_u64);
    let cache_hit_tokens = usage.get("prompt_cache_hit_tokens").and_then(Value::as_u64).or(nested_cached_tokens);
    let cache_miss_tokens = usage.get("prompt_cache_miss_tokens").and_then(Value::as_u64).or_else(|| match (prompt_tokens, cache_hit_tokens) {
        (Some(prompt), Some(hit)) => prompt.checked_sub(hit),
        _ => None,
    });
    let tokens_per_second = usage.get("tokens_per_second").and_then(Value::as_f64);
    let time_to_first_token_ms = usage.get("time_to_first_token_ms").and_then(Value::as_f64);
    let time_per_output_token_ms = usage.get("time_per_output_token_ms").and_then(Value::as_f64);

    let parsed = TokenUsage {
        prompt_tokens,
        completion_tokens,
        total_tokens,
        cache_hit_tokens,
        cache_miss_tokens,
        tokens_per_second,
        time_to_first_token_ms,
        time_per_output_token_ms,
    };

    (parsed != TokenUsage::default()).then_some(parsed)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_sse_usage_chunk() {
        let usage = parse_usage_from_sse_chunk(
            br#"data: {"usage":{"prompt_tokens":30,"completion_tokens":90,"total_tokens":120}}

data: [DONE]
"#,
        )
        .unwrap();

        assert_eq!(usage.prompt_tokens, Some(30));
        assert_eq!(usage.completion_tokens, Some(90));
        assert_eq!(usage.total_tokens, Some(120));
    }
}
