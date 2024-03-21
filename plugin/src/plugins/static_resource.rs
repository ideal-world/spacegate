use std::path::PathBuf;

use serde_json::Value;

pub struct StaticResourceConfig {
    pub code: u16,
    pub content_type: String,
    pub body: BodyEnum,
}

pub enum BodyEnum {
    Json(Value),
    Text(String),
    File(PathBuf),
}
