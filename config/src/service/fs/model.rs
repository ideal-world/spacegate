use serde_json::Value;
use spacegate_model::PluginInstanceId;

pub enum PluginConfigEnum {
    Anon { uid: Option<u64>, code: String, spec: Value },
    Named { name: String, code: String },
    Mono { code: String },
}
