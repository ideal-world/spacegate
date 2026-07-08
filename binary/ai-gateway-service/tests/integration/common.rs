pub use ai_gateway_service::TestHarness;

pub fn small_body() -> Vec<u8> {
    br#"{"model":"gpt-4","messages":[{"role":"user","content":"hi"}]}"#.to_vec()
}

pub async fn parse_rate_limit(resp: reqwest::Response) -> serde_json::Value {
    resp.json().await.expect("rate limit json")
}
