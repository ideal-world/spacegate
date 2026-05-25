use spacegate_plugin::{set_plugin_telemetry_field, set_telemetry_field, SgBody};

fn request_with_telemetry() -> hyper::Request<SgBody> {
    let mut req = hyper::Request::builder().body(SgBody::empty()).expect("request");
    req.extensions_mut().insert(spacegate_kernel::observability::TelemetryContext::default());
    req
}

#[test]
fn set_telemetry_field_writes_checked_request_context() {
    let req = request_with_telemetry();

    set_telemetry_field(&req, "ai.asset_id", "deepseek-chat").expect("insert");
    set_telemetry_field(&req, "ai.total_tokens", 37).expect("insert");

    let fields = req.extensions().get::<spacegate_kernel::observability::TelemetryContext>().expect("telemetry context").snapshot();
    assert_eq!(fields.get("ai.asset_id").map(String::as_str), Some("deepseek-chat"));
    assert_eq!(fields.get("ai.total_tokens").map(String::as_str), Some("37"));
}

#[test]
fn set_plugin_telemetry_field_adds_namespace() {
    let req = request_with_telemetry();

    set_plugin_telemetry_field(&req, "mcp", "tool", "search").expect("insert");

    let fields = req.extensions().get::<spacegate_kernel::observability::TelemetryContext>().expect("telemetry context").snapshot();
    assert_eq!(fields.get("mcp.tool").map(String::as_str), Some("search"));
}

#[test]
fn set_telemetry_field_rejects_unqualified_key() {
    let req = request_with_telemetry();

    let result = set_telemetry_field(&req, "total_tokens", 37);

    assert_eq!(result, Err(spacegate_kernel::observability::TelemetryError::MissingNamespace));
}
