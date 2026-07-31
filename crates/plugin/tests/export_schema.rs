use spacegate_plugin::{ext::redis::plugins as redis_plugins, plugins, Plugin, PluginSchemaExt};

fn export_plugin<P: PluginSchemaExt + Plugin>(dir: std::path::PathBuf) {
    let schema = <P as spacegate_plugin::PluginSchemaExt>::schema();
    let json = serde_json::to_string_pretty(&schema).unwrap();
    let filename = format!("{}.json", P::CODE);
    let path = dir.join(filename);
    std::fs::write(path, json).unwrap();
}

macro_rules! export_plugins {
    ($path: literal : $($plugin:ty)*) => {
        let dir = std::path::PathBuf::from($path);
        std::fs::create_dir_all(&dir).unwrap();
        $(export_plugin::<$plugin>(dir.clone());)*
    };
}

#[test]
fn export_schema() {
    use plugins::{
        east_west_traffic_white_list::EastWestTrafficWhiteListPlugin,
        header_modifier::HeaderModifierPlugin,
        inject::InjectPlugin,
        limit::RateLimitPlugin,
        maintenance::MaintenancePlugin,
        redirect::RedirectPlugin,
        // retry::RetryPlugin,
        rewrite::RewritePlugin,
        set_scheme::SetSchemePlugin,
        set_version::SetVersionPlugin,
        static_resource::StaticResourcePlugin,
    };
    use redis_plugins::{redis_count::RedisCountPlugin, redis_dynamic_route::RedisDynamicRoutePlugin, redis_limit::RedisLimitPlugin, redis_time_range::RedisTimeRangePlugin};
    export_plugins!("schema":
        HeaderModifierPlugin
        InjectPlugin
        RateLimitPlugin
        MaintenancePlugin
        RedirectPlugin
        // RetryPlugin
        RewritePlugin
        SetSchemePlugin
        SetVersionPlugin
        StaticResourcePlugin
        EastWestTrafficWhiteListPlugin
        RedisCountPlugin
        RedisDynamicRoutePlugin
        RedisLimitPlugin
        RedisTimeRangePlugin
    );
}

fn assert_has_populated_example<P: PluginSchemaExt>() {
    let schema = serde_json::to_value(P::schema()).expect("serialize plugin schema");
    let example = schema.get("examples").and_then(|examples| examples.as_array()).and_then(|examples| examples.first()).expect("plugin schema must expose an example");
    assert!(example.is_object(), "plugin schema example must be an object");
    assert!(
        example.as_object().expect("object example").values().all(|value| !value.is_null()),
        "plugin schema example must not contain null values"
    );
}

fn schema_example<P: PluginSchemaExt>() -> serde_json::Value {
    serde_json::to_value(P::schema())
        .expect("serialize plugin schema")
        .get("examples")
        .and_then(|examples| examples.as_array())
        .and_then(|examples| examples.first())
        .cloned()
        .expect("plugin schema must expose an example")
}

fn assert_example_is_parseable<P: Plugin + PluginSchemaExt>() {
    P::create_by_spec(schema_example::<P>(), spacegate_model::PluginInstanceName::named("schema-example")).expect("plugin schema example must be accepted by the plugin");
}

#[test]
fn native_plugin_schema_examples_are_populated() {
    use plugins::{
        east_west_traffic_white_list::EastWestTrafficWhiteListPlugin, header_modifier::HeaderModifierPlugin, inject::InjectPlugin, limit::RateLimitPlugin,
        maintenance::MaintenancePlugin, redirect::RedirectPlugin, rewrite::RewritePlugin, set_scheme::SetSchemePlugin, set_version::SetVersionPlugin,
        static_resource::StaticResourcePlugin,
    };
    use redis_plugins::{redis_count::RedisCountPlugin, redis_dynamic_route::RedisDynamicRoutePlugin, redis_limit::RedisLimitPlugin, redis_time_range::RedisTimeRangePlugin};

    assert_has_populated_example::<StaticResourcePlugin>();
    assert_has_populated_example::<RateLimitPlugin>();
    assert_has_populated_example::<RedirectPlugin>();
    assert_has_populated_example::<HeaderModifierPlugin>();
    assert_has_populated_example::<InjectPlugin>();
    assert_has_populated_example::<RewritePlugin>();
    assert_has_populated_example::<MaintenancePlugin>();
    assert_has_populated_example::<SetVersionPlugin>();
    assert_has_populated_example::<SetSchemePlugin>();
    assert_has_populated_example::<EastWestTrafficWhiteListPlugin>();
    assert_has_populated_example::<RedisCountPlugin>();
    assert_has_populated_example::<RedisDynamicRoutePlugin>();
    assert_has_populated_example::<RedisLimitPlugin>();
    assert_has_populated_example::<RedisTimeRangePlugin>();
}

#[test]
fn native_plugin_schema_examples_are_parseable() {
    use plugins::{
        east_west_traffic_white_list::EastWestTrafficWhiteListPlugin, header_modifier::HeaderModifierPlugin, inject::InjectPlugin, limit::RateLimitPlugin,
        maintenance::MaintenancePlugin, redirect::RedirectPlugin, rewrite::RewritePlugin, set_scheme::SetSchemePlugin, set_version::SetVersionPlugin,
        static_resource::StaticResourcePlugin,
    };
    use redis_plugins::{redis_count::RedisCountPlugin, redis_dynamic_route::RedisDynamicRoutePlugin, redis_limit::RedisLimitPlugin, redis_time_range::RedisTimeRangePlugin};

    assert_example_is_parseable::<StaticResourcePlugin>();
    assert_example_is_parseable::<RateLimitPlugin>();
    assert_example_is_parseable::<RedirectPlugin>();
    assert_example_is_parseable::<HeaderModifierPlugin>();
    assert_example_is_parseable::<InjectPlugin>();
    assert_example_is_parseable::<RewritePlugin>();
    assert_example_is_parseable::<MaintenancePlugin>();
    assert_example_is_parseable::<SetVersionPlugin>();
    assert_example_is_parseable::<SetSchemePlugin>();
    assert_example_is_parseable::<EastWestTrafficWhiteListPlugin>();
    assert_example_is_parseable::<RedisCountPlugin>();
    assert_example_is_parseable::<RedisDynamicRoutePlugin>();
    assert_example_is_parseable::<RedisLimitPlugin>();
    assert_example_is_parseable::<RedisTimeRangePlugin>();
}
