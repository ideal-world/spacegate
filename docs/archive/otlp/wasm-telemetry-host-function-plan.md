# WASM 插件审计字段 Host Function 技术方案

## 目标

让 WASM 插件也能像原生插件一样写入请求级业务审计字段，例如：

- `ai.asset_id`
- `ai.asset_type`
- `ai.prompt_tokens`
- `ai.completion_tokens`
- `ai.total_tokens`
- `auth.app_id`
- `auth.api_key_hash`
- `mcp.server`
- `mcp.tool`
- `mcp.success`
- `error.code`

这些字段最终随请求结束时的 `http_access` 日志进入 OTLP logs，再由 Collector 写入 ClickHouse 的 `otel_logs`。

## 当前原生插件链路

原生插件调用：

```rust
spacegate_plugin::set_plugin_telemetry_field(&req, "ai", "asset_id", "deepseek-chat")?;
spacegate_plugin::set_plugin_telemetry_field(&req, "ai", "total_tokens", 37)?;
```

数据流：

```text
SgRequest.extensions.TelemetryContext
  -> kernel 请求结束生成 http_access 日志
  -> telemetry JSON log attribute
  -> OTLP logs
  -> Collector
  -> ClickHouse otel_logs
```

## 推荐 WASM ABI

新增非 proxy-wasm 标准的 SpaceGate 扩展 host function：

```text
env.spacegate_set_telemetry_field(key_ptr, key_len, value_ptr, value_len) -> status
```

参数：

- `key_ptr: i32`
- `key_len: i32`
- `value_ptr: i32`
- `value_len: i32`

返回：

- `Status::Ok`
- `Status::BadArgument`
- `Status::InvalidMemoryAccess`
- `Status::NotFound`

命名选择：

- 不复用 `proxy_call_foreign_function`，避免把核心审计能力塞进不透明 FFI。
- 使用 `spacegate_` 前缀，明确这是 SpaceGate 扩展，不污染 proxy-wasm 标准 ABI。

## Host 侧实现

### 1. HostState 增加请求级 telemetry 存储

在 `crates/plugin-wasm/src/host_state.rs` 的 `RequestContext` 增加：

```rust
pub telemetry_fields: BTreeMap<String, String>,
```

原因：

- WASM `Vm::process` 目前会把 `SgRequest` 拆成 `parts/body`，再重建 `new_req` 给 `inner.call`。
- host fn 执行期间拿不到原始 `SgRequest` 引用。
- 因此 WASM 调 host fn 时先写到当前 `RequestContext`，请求结束或调用 inner 前再同步到 `SgRequest.extensions.TelemetryContext`。

### 2. 注册 host function

在 `crates/plugin-wasm/src/host_fn.rs` 增加：

```rust
fn register_spacegate_telemetry(linker: &mut Linker<HostState>) -> Result<(), wasmtime::Error>
```

并在 `register_all` 中调用。

处理逻辑：

1. 用 `MemoryHelper::from_caller` 读取 guest memory。
2. 读取 `key` 和 `value` 字符串。
3. 校验：
   - key 非空
   - key 最大 128 字节
   - value 最大 4096 字节
   - key 必须包含命名空间分隔符 `.`
   - key 只能包含 `[A-Za-z0-9_.-]`
   - 禁止保留前缀：`http.`、`net.`、`gateway.`、`spacegate.`、`otel.`
4. 获取 `caller.data_mut().current_context_mut()`。
5. 写入 `ctx.telemetry_fields.insert(key, value)`。
6. 返回 `Status::Ok`。

### 3. 同步到 SgRequest

在 `crates/plugin-wasm/src/vm.rs` 的 `Vm::process` 中：

- 重建 `new_req` 后、调用 `inner.call(new_req).await` 前：

```rust
if let Some(kernel_ctx) = new_req.extensions().get::<spacegate_kernel::observability::TelemetryContext>() {
    for (key, value) in wasm_ctx.telemetry_fields {
        kernel_ctx.insert_checked(key, value)?;
    }
}
```

注意：

- 需要把 `let new_req = ...` 改成 `let mut new_req = ...` 或在构造前保留 extensions。
- 当前 request parts 来自原始 `SgRequest`，extensions 会保留，所以 kernel 插入的 `TelemetryContext` 可以继续存在。

### 4. 本地响应短路场景

如果 WASM 在 request 阶段通过 `proxy_send_local_response` 直接返回，不会调用 `inner.call`。

这种情况下也需要把 telemetry 同步回 access log：

- 方案 A：短路前直接从原始 request extensions 同步。
- 方案 B：在 `Vm::process` 开始时把 `TelemetryContext` clone 存进 `HostState` 或当前 `RequestContext`。

推荐方案 B：

```rust
RequestContext {
    telemetry_sink: Option<TelemetryContext>,
}
```

在 `Vm::process` 开始时：

```rust
let telemetry_sink = parts.extensions.get::<TelemetryContext>().cloned();
ctx.telemetry_sink = telemetry_sink;
```

host fn 写字段时：

```rust
if let Some(sink) = &ctx.telemetry_sink {
    sink.insert_checked(key.clone(), value.clone())?;
}
ctx.telemetry_fields.insert(key, value);
```

这样即使本地响应短路，kernel 请求结束时也能读到审计字段。

## Guest SDK 封装

WASM 插件侧建议提供一个薄封装：

```rust
#[link(wasm_import_module = "env")]
extern "C" {
    fn spacegate_set_telemetry_field(
        key_ptr: i32,
        key_len: i32,
        value_ptr: i32,
        value_len: i32,
    ) -> i32;
}

pub fn set_telemetry_field(key: &str, value: impl ToString) -> Result<(), Status> {
    let value = value.to_string();
    let status = unsafe {
        spacegate_set_telemetry_field(
            key.as_ptr() as i32,
            key.len() as i32,
            value.as_ptr() as i32,
            value.len() as i32,
        )
    };
    Status::from_i32(status)
}

pub fn set_plugin_telemetry_field(namespace: &str, key: &str, value: impl ToString) -> Result<(), Status> {
    set_telemetry_field(&format!("{namespace}.{key}"), value)
}
```

插件使用：

```rust
set_plugin_telemetry_field("ai", "asset_id", "deepseek-chat")?;
set_plugin_telemetry_field("ai", "prompt_tokens", 24)?;
set_plugin_telemetry_field("ai", "completion_tokens", 13)?;
set_plugin_telemetry_field("ai", "total_tokens", 37)?;
set_plugin_telemetry_field("mcp", "tool", "search")?;
```

## 测试计划

### 单元测试

- `host_fn` 能读取 guest memory 中的 key/value。
- 非法 key 返回 `BadArgument`。
- 空 key 返回 `BadArgument`。
- 超长 value 返回 `BadArgument`。
- 无当前 HTTP context 返回 `NotFound`。

### WASM 集成测试

新增一个测试 wasm：

- 在 `proxy_on_request_headers` 写 `ai.asset_id`。
- 在 `proxy_on_response_body` 写 token 字段。
- 请求结束后断言 `TelemetryContext.snapshot()` 包含这些字段。

### 端到端测试

本地脚本启动：

```bash
scripts/otel-local/start-clickhouse.sh
scripts/otel-local/start-collector.sh
scripts/otel-local/start-mock-ac.sh
scripts/otel-local/start-spacegate.sh
scripts/otel-local/request.sh
scripts/otel-local/query-access-logs.sh
```

确认 ClickHouse `otel_logs` 中包含：

```text
JSONExtractString(LogAttributes['telemetry'], 'ai.asset_id')
JSONExtractString(LogAttributes['telemetry'], 'ai.total_tokens')
JSONExtractString(LogAttributes['telemetry'], 'mcp.tool')
```

## 风险与边界

- 这是 SpaceGate 扩展 ABI，不是 proxy-wasm 标准函数。
- 字段 key 必须限制字符集和长度，避免 ClickHouse 查询侧难以治理。
- 不建议把 `request_id`、用户 ID、完整 prompt、完整 response body 写入 telemetry 字段。
- token、MCP、模型 ID 属于审计日志字段，不应作为 metrics label。
- WASM 当前单 VM 串行处理请求，字段必须存放在 `RequestContext`，不能放全局 map。
