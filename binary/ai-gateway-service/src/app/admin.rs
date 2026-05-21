const AI_GATEWAY_QUEUE_PLUGIN: &str = "ai-gateway-queue";
const AI_GATEWAY_QUEUE_README: &str = include_str!("../../../../plugins/wasm/ai-gateway-queue/README.md");

async fn admin_plugin_schema(Path(plugin): Path<String>) -> Result<Json<serde_json::Value>, ServiceError> {
    if plugin != AI_GATEWAY_QUEUE_PLUGIN {
        return Err(ServiceError::bad_request(format!("unsupported plugin `{plugin}`")));
    }

    let schema = schema_for!(AiGatewayQueuePluginConfig);
    let mut value = serde_json::to_value(&schema).map_err(|e| ServiceError::internal(format!("serialize schema: {e}")))?;
    add_ai_gateway_queue_schema_extensions(&mut value);
    Ok(Json(value))
}

async fn admin_plugin_readme(Path(plugin): Path<String>) -> Result<Response, ServiceError> {
    if plugin != AI_GATEWAY_QUEUE_PLUGIN {
        return Err(ServiceError::bad_request(format!("unsupported plugin `{plugin}`")));
    }
    Ok((StatusCode::OK, [("content-type", "text/markdown; charset=utf-8")], AI_GATEWAY_QUEUE_README).into_response())
}

async fn admin_list_tenant_rate_limits(State(state): State<AppState>, Query(filters): Query<HashMap<String, String>>) -> Result<Json<Vec<TenantRateLimitRuleView>>, ServiceError> {
    let rules = list_tenant_rate_limit_rules(&state, &filters).await?;
    Ok(Json(rules))
}

async fn admin_upsert_tenant_rate_limit(State(state): State<AppState>, Json(rule): Json<TenantRateLimitRule>) -> Result<Json<TenantRateLimitRuleView>, ServiceError> {
    let rule = upsert_tenant_rate_limit_rule(&state, rule).await?;
    Ok(Json(rule))
}

async fn admin_delete_tenant_rate_limit(State(state): State<AppState>, Json(rule): Json<TenantRateLimitRule>) -> Result<Json<serde_json::Value>, ServiceError> {
    let removed = delete_tenant_rate_limit_rule(&state, rule).await?;
    Ok(Json(serde_json::json!({ "deleted": removed })))
}

fn add_ai_gateway_queue_schema_extensions(value: &mut serde_json::Value) {
    let example = serde_json::to_string_pretty(&AiGatewayQueuePluginConfig::default()).unwrap_or_default();
    if let Some(object) = value.as_object_mut() {
        object.insert("x-example-raw".to_string(), serde_json::Value::String(example));
        object.insert(
            "x-title-i18n".to_string(),
            serde_json::json!({
                "zh-CN": "AI 请求队列网关",
                "en": "AI Request Queue Gateway"
            }),
        );
        object.insert(
            "x-description-i18n".to_string(),
            serde_json::json!({
                "zh-CN": "配置 AI 请求队列网关：队列后端接入、入队接口路径、请求头映射、队列模式与优先级路由。",
                "en": "Configure the AI request queue gateway: queue backend access, enqueue paths, header mapping, queue mode and priority routing."
            }),
        );
    }

    let Some(definitions) = value.get_mut("definitions").and_then(|v| v.as_object_mut()) else { return };

    // 子配置卡片自身的标题/说明：会被 SchemaForm 的 el-card 标题区使用。
    annotate_schema_meta(
        definitions.get_mut("AiGatewayServiceConfig"),
        "队列后端接入",
        "Queue Backend Access",
        "Wasm 插件调用外部队列后端时使用的 cluster、authority 和超时设置。",
        "Cluster, authority and timeout the wasm plugin uses to call the external queue backend.",
    );
    annotate_schema_meta(
        definitions.get_mut("AiGatewayPathsConfig"),
        "接口路径",
        "Paths",
        "队列后端暴露的准入判定、入队、入队并等待三类 HTTP 路径。",
        "HTTP paths exposed by the queue backend for admission check, enqueue, and enqueue-and-wait.",
    );
    annotate_schema_meta(
        definitions.get_mut("AiGatewayHeadersConfig"),
        "请求头映射",
        "Headers",
        "客户端实际使用的 Header 名称；插件会把它们统一转成队列后端期望的标准 Header。",
        "Header names used by clients; the plugin remaps them to the standard headers the queue backend expects.",
    );
    annotate_schema_meta(
        definitions.get_mut("AiGatewayPoliciesConfig"),
        "队列模式",
        "Queue Mode",
        "控制 X-RateLimit-Policy 请求头是否必填，以及未携带时使用的默认队列模式（abandon / queue / wait）。",
        "Controls whether the X-RateLimit-Policy header is required, and the default queue mode used when it is missing (abandon / queue / wait).",
    );
    annotate_schema_meta(
        definitions.get_mut("AiGatewayPriorityConfig"),
        "优先级路由",
        "Priority Routing",
        "队列优先级的开关、默认值，以及按模型 / 租户自动选择高 / 普通 / 低优先级队列的规则。",
        "Queue priority switch, default value, and per-model / per-tenant rules that route requests into high / normal / low priority streams.",
    );

    set_field_descriptions(
        definitions.get_mut("AiGatewayServiceConfig"),
        &[
            (
                "cluster",
                "队列后端 Cluster",
                "Queue Backend Cluster",
                "SpaceGate 中指向队列后端的 cluster 名称，对应 SpaceGate 配置里的 clusters 键。",
                "Name of the SpaceGate cluster pointing to the queue backend; matches the key under the clusters field.",
            ),
            (
                "authority",
                "队列后端 Authority",
                "Queue Backend Authority",
                "Wasm 插件 dispatch HTTP call 时使用的 :authority，通常和 cluster 同名。",
                "The :authority used by the wasm dispatch_http_call; usually the same as the cluster name.",
            ),
            (
                "timeout_ms",
                "调用超时（毫秒）",
                "Timeout (ms)",
                "调用队列后端的超时时间；wait 模式需要留足同步等待时间，建议 60000 ms 以上。",
                "Timeout for calling the queue backend. Keep it above 60000 ms when wait mode is used.",
            ),
        ],
    );

    set_field_descriptions(
        definitions.get_mut("AiGatewayPathsConfig"),
        &[
            (
                "rate_limit",
                "准入判定路径",
                "Admission Check Path",
                "队列后端用于判断请求是否需要入队的准入接口，默认 /v1/ratelimit/check。",
                "Backend path that decides whether a request should be enqueued. Default: /v1/ratelimit/check.",
            ),
            (
                "enqueue",
                "入队路径",
                "Enqueue Path",
                "queue 模式使用的异步入队接口，默认 /v1/queue/enqueue。",
                "Endpoint used by the queue (async) mode. Default: /v1/queue/enqueue.",
            ),
            (
                "wait",
                "入队并等待路径",
                "Enqueue-and-Wait Path",
                "wait 模式使用的入队并同步等待结果接口，默认 /v1/queue/enqueue-and-wait。",
                "Endpoint used by the wait (sync) mode. Default: /v1/queue/enqueue-and-wait.",
            ),
        ],
    );

    set_field_descriptions(
        definitions.get_mut("AiGatewayHeadersConfig"),
        &[
            (
                "policy",
                "队列模式 Header",
                "Queue Mode Header",
                "客户端用于声明队列模式（abandon / queue / wait）的 Header，插件会转成后端使用的 x-ratelimit-policy。",
                "Header the client uses to declare the queue mode (abandon / queue / wait); remapped to x-ratelimit-policy.",
            ),
            (
                "tenant",
                "租户 Header",
                "Tenant Header",
                "客户端表示租户身份的 Header，插件会转成队列后端使用的 x-tenant-id。",
                "Header carrying tenant identity; remapped to x-tenant-id for the queue backend.",
            ),
            (
                "model",
                "模型 Header",
                "Model Header",
                "客户端声明目标模型的 Header，会被透传为队列后端使用的 x-model。",
                "Header that names the target model; remapped to x-model for the queue backend.",
            ),
            (
                "priority",
                "优先级 Header",
                "Priority Header",
                "客户端可选的队列优先级 Header，启用优先级时会被转为 x-queue-priority（取值 high/normal/low）。",
                "Optional header for queue priority, remapped to x-queue-priority (values high/normal/low).",
            ),
        ],
    );

    set_field_descriptions(
        definitions.get_mut("AiGatewayPoliciesConfig"),
        &[
            (
                "require",
                "强制要求队列模式 Header",
                "Require Queue Mode Header",
                "为 true 时，请求未携带队列模式 Header 会直接返回 400；关闭后会回退到默认队列模式。",
                "When true, requests without the queue-mode header are rejected with 400; otherwise falls back to the default mode.",
            ),
            (
                "default",
                "默认队列模式",
                "Default Queue Mode",
                "未携带队列模式 Header 且 require 为 false 时使用的默认模式，可选 abandon / queue / wait。",
                "Default queue mode when require is false and the request omits the header. One of abandon / queue / wait.",
            ),
        ],
    );

    set_field_descriptions(
        definitions.get_mut("AiGatewayPriorityConfig"),
        &[
            (
                "enabled",
                "启用优先级路由",
                "Enable Priority Routing",
                "总开关：关闭后所有请求都进入 normal 优先级队列，不再读取模型/租户规则。",
                "Master switch; when disabled, all requests go to the normal-priority queue and per-model / per-tenant rules are ignored.",
            ),
            (
                "default",
                "默认队列优先级",
                "Default Queue Priority",
                "命中不到任何规则时使用的默认队列优先级，可选 high / normal / low。",
                "Default queue priority used when no rule matches. One of high / normal / low.",
            ),
            (
                "high_models",
                "高优队列模型列表",
                "High Priority Models",
                "命中后自动路由到高优队列的模型名列表（精确匹配，区分大小写）。",
                "Models that are routed to the high-priority queue (exact, case-sensitive match).",
            ),
            (
                "low_models",
                "低优队列模型列表",
                "Low Priority Models",
                "命中后自动路由到低优队列的模型名列表。",
                "Models that are routed to the low-priority queue.",
            ),
            (
                "high_tenants",
                "高优队列租户列表",
                "High Priority Tenants",
                "命中后自动路由到高优队列的租户 ID 列表。",
                "Tenant IDs that are routed to the high-priority queue.",
            ),
            (
                "low_tenants",
                "低优队列租户列表",
                "Low Priority Tenants",
                "命中后自动路由到低优队列的租户 ID 列表，常用于免费 / 试用租户。",
                "Tenant IDs that are routed to the low-priority queue, typically used for free or trial tenants.",
            ),
        ],
    );
}

fn annotate_schema_meta(schema: Option<&mut serde_json::Value>, zh_title: &str, en_title: &str, zh_desc: &str, en_desc: &str) {
    let Some(object) = schema.and_then(|v| v.as_object_mut()) else { return };
    object.insert(
        "x-title-i18n".to_string(),
        serde_json::json!({ "zh-CN": zh_title, "en": en_title }),
    );
    object.insert(
        "x-description-i18n".to_string(),
        serde_json::json!({ "zh-CN": zh_desc, "en": en_desc }),
    );
}

fn set_field_descriptions(schema: Option<&mut serde_json::Value>, items: &[(&str, &str, &str, &str, &str)]) {
    let Some(properties) = schema.and_then(|v| v.get_mut("properties")).and_then(|v| v.as_object_mut()) else { return };
    for (key, zh_title, en_title, zh_desc, en_desc) in items {
        let Some(field) = properties.get_mut(*key).and_then(|v| v.as_object_mut()) else { continue };
        field.insert(
            "x-title-i18n".to_string(),
            serde_json::json!({ "zh-CN": zh_title, "en": en_title }),
        );
        field.insert(
            "x-description-i18n".to_string(),
            serde_json::json!({ "zh-CN": zh_desc, "en": en_desc }),
        );
    }
}
