# Telemetry Pluginized Audit Fields Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 将请求级业务审计字段改造成真正插件化的 `TelemetryContext`：插件按命名空间写入字段，网关只负责统一携带、校验、序列化到 access log，并通过 OTLP logs 入库。

**Architecture:** `kernel` 提供请求级 `TelemetryContext` 和字段校验/序列化能力；`plugin` 提供原生插件写入 API；`service.rs` 只输出通用 access log 字段和一个 JSON 字符串 `telemetry`，不再硬编码 AI/MCP/token 等业务字段。ClickHouse 查询侧从 `LogAttributes['telemetry']` JSON 解析插件自定义字段。

**Tech Stack:** Rust, hyper request extensions, tracing structured logs, OpenTelemetry logs, ClickHouse `Map`/JSON extraction.

---

## 设计补充与风险点

当前方向基本正确，但还需要明确这些边界：

- **命名空间冲突**：A/B 插件不能直接共用 `total_tokens` 这类裸 key，推荐 `ai.total_tokens`、`mcp.tool`、`auth.app_id`。
- **保留前缀**：禁止插件写 `http.*`、`net.*`、`gateway.*`、`spacegate.*`、`otel.*`，避免和网关/OTEL 主字段混淆。
- **字段结构**：`TelemetryContext` 保持扁平 `key/value`，不接受嵌套 JSON 对象，避免合并语义和查询复杂度失控。
- **字段长度**：限制 key/value 大小，防止插件误写完整 prompt、response body 或超大错误堆栈。
- **覆盖策略**：同 key 后写覆盖前写；这是同命名空间内插件自己的责任。跨插件通过 namespace 避免冲突。
- **敏感信息**：不建议写完整 `api_key`，推荐写 `api_key_hash` 或脱敏值。
- **metrics 边界**：业务审计字段只进入 logs/traces，不作为 metrics label，避免高基数。
- **ClickHouse 性能**：低频查询可直接 `JSONExtract*`；高频统计建议建物化视图抽取常用字段。
- **WASM 对齐**：WASM host function 也必须遵守同一套 key 校验、namespace 和长度限制。

## File Structure

- Modify: `crates/kernel/src/observability.rs`
  - 定义 telemetry 字段校验规则。
  - 提供 `TelemetryError`。
  - 提供 `TelemetryContext::insert_checked`。
  - 提供 `TelemetryContext::insert_namespaced`。
  - 提供 `telemetry_json`.

- Modify: `crates/kernel/src/service.rs`
  - 删除硬编码 `telemetry.asset_id`、`telemetry.total_tokens` 等业务字段。
  - access log 只输出一个 `telemetry` JSON 字符串。

- Modify: `crates/plugin/src/lib.rs`
  - 保留 `set_telemetry_field`，内部走 checked insert。
  - 新增推荐 API `set_plugin_telemetry_field(req, namespace, key, value)`。
  - 返回 `Result<(), TelemetryError>`，让插件可感知字段被拒绝。

- Modify: `crates/plugin/tests/test_telemetry.rs`
  - 覆盖原生插件 API、命名空间 API、非法 key、保留前缀。

- Modify: `scripts/otel-local/query-access-logs.sh`
  - 从 `LogAttributes['telemetry']` JSON 中解析字段，不再查询 `LogAttributes['telemetry.asset_id']`。

- Modify: `docs/archive/otlp/wasm-telemetry-host-function-plan.md`
  - 对齐本计划中的校验规则和 telemetry JSON 入库形态。

---

### Task 1: Kernel Telemetry Validation

**Files:**
- Modify: `crates/kernel/src/observability.rs`

- [ ] **Step 1: Write failing tests for validation**

Add these tests inside `#[cfg(test)] mod tests` in `crates/kernel/src/observability.rs`:

```rust
#[test]
fn telemetry_key_validation_accepts_namespaced_keys() {
    assert!(super::validate_telemetry_key("ai.total_tokens").is_ok());
    assert!(super::validate_telemetry_key("mcp.tool-name").is_ok());
    assert!(super::validate_telemetry_key("auth.api_key_hash").is_ok());
}

#[test]
fn telemetry_key_validation_rejects_bad_keys() {
    assert_eq!(super::validate_telemetry_key(""), Err(super::TelemetryError::EmptyKey));
    assert_eq!(super::validate_telemetry_key("total_tokens"), Err(super::TelemetryError::MissingNamespace));
    assert_eq!(super::validate_telemetry_key("ai total_tokens"), Err(super::TelemetryError::InvalidKey));
    assert_eq!(super::validate_telemetry_key("http.status_code"), Err(super::TelemetryError::ReservedPrefix));
    assert_eq!(super::validate_telemetry_key("spacegate.internal"), Err(super::TelemetryError::ReservedPrefix));
}

#[test]
fn telemetry_value_validation_rejects_oversized_values() {
    let value = "x".repeat(super::MAX_TELEMETRY_VALUE_LEN + 1);
    assert_eq!(super::validate_telemetry_value(&value), Err(super::TelemetryError::ValueTooLong));
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run:

```bash
cargo test -p spacegate-kernel telemetry_key_validation 2>&1 | head -c 12000
```

Expected: FAIL because `validate_telemetry_key`, `validate_telemetry_value`, `TelemetryError`, or `MAX_TELEMETRY_VALUE_LEN` are missing.

- [ ] **Step 3: Implement validation**

Add near `TelemetryContext` in `crates/kernel/src/observability.rs`:

```rust
pub const MAX_TELEMETRY_KEY_LEN: usize = 128;
pub const MAX_TELEMETRY_VALUE_LEN: usize = 4096;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TelemetryError {
    EmptyKey,
    MissingNamespace,
    ReservedPrefix,
    InvalidKey,
    KeyTooLong,
    ValueTooLong,
}

pub fn validate_telemetry_key(key: &str) -> Result<(), TelemetryError> {
    if key.is_empty() {
        return Err(TelemetryError::EmptyKey);
    }
    if key.len() > MAX_TELEMETRY_KEY_LEN {
        return Err(TelemetryError::KeyTooLong);
    }
    if !key.contains('.') {
        return Err(TelemetryError::MissingNamespace);
    }
    if ["http.", "net.", "gateway.", "spacegate.", "otel."].iter().any(|prefix| key.starts_with(prefix)) {
        return Err(TelemetryError::ReservedPrefix);
    }
    if !key.bytes().all(|b| b.is_ascii_alphanumeric() || matches!(b, b'.' | b'_' | b'-')) {
        return Err(TelemetryError::InvalidKey);
    }
    Ok(())
}

pub fn validate_telemetry_value(value: &str) -> Result<(), TelemetryError> {
    if value.len() > MAX_TELEMETRY_VALUE_LEN {
        return Err(TelemetryError::ValueTooLong);
    }
    Ok(())
}
```

- [ ] **Step 4: Run tests to verify they pass**

Run:

```bash
cargo test -p spacegate-kernel telemetry_key_validation 2>&1 | head -c 12000
cargo test -p spacegate-kernel telemetry_value_validation 2>&1 | head -c 12000
```

Expected: PASS.

---

### Task 2: Checked TelemetryContext API

**Files:**
- Modify: `crates/kernel/src/observability.rs`

- [ ] **Step 1: Write failing tests for checked insertion**

Add tests:

```rust
#[test]
fn telemetry_context_checked_insert_rejects_invalid_key_without_mutating_context() {
    let context = super::TelemetryContext::default();

    let result = context.insert_checked("total_tokens", "37");

    assert_eq!(result, Err(super::TelemetryError::MissingNamespace));
    assert!(context.snapshot().is_empty());
}

#[test]
fn telemetry_context_namespaced_insert_builds_stable_key() {
    let context = super::TelemetryContext::default();

    context.insert_namespaced("ai", "total_tokens", 37).expect("insert");

    let fields = context.snapshot();
    assert_eq!(fields.get("ai.total_tokens").map(String::as_str), Some("37"));
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run:

```bash
cargo test -p spacegate-kernel telemetry_context_checked_insert 2>&1 | head -c 12000
cargo test -p spacegate-kernel telemetry_context_namespaced_insert 2>&1 | head -c 12000
```

Expected: FAIL because methods are missing.

- [ ] **Step 3: Implement checked APIs**

Replace or extend `impl TelemetryContext` with:

```rust
impl TelemetryContext {
    pub fn insert(&self, key: impl Into<String>, value: impl Into<String>) {
        let Ok(mut fields) = self.fields.lock() else {
            return;
        };
        fields.insert(key.into(), value.into());
    }

    pub fn insert_checked(&self, key: impl Into<String>, value: impl ToString) -> Result<(), TelemetryError> {
        let key = key.into();
        let value = value.to_string();
        validate_telemetry_key(&key)?;
        validate_telemetry_value(&value)?;
        let Ok(mut fields) = self.fields.lock() else {
            return Ok(());
        };
        fields.insert(key, value);
        Ok(())
    }

    pub fn insert_namespaced(&self, namespace: &str, key: &str, value: impl ToString) -> Result<(), TelemetryError> {
        self.insert_checked(format!("{namespace}.{key}"), value)
    }

    pub fn snapshot(&self) -> BTreeMap<String, String> {
        self.fields.lock().map(|fields| fields.clone()).unwrap_or_default()
    }

    pub fn is_empty(&self) -> bool {
        self.fields.lock().map(|fields| fields.is_empty()).unwrap_or(true)
    }
}
```

- [ ] **Step 4: Run tests**

Run:

```bash
cargo test -p spacegate-kernel telemetry_context_ 2>&1 | head -c 12000
```

Expected: PASS.

---

### Task 3: Access Log Uses Generic Telemetry JSON

**Files:**
- Modify: `crates/kernel/src/observability.rs`
- Modify: `crates/kernel/src/service.rs`

- [ ] **Step 1: Write failing JSON serialization test**

Add test:

```rust
#[test]
fn telemetry_json_serializes_plugin_defined_fields() {
    let fields = BTreeMap::from([
        ("ai.asset_id".to_string(), "deepseek-chat".to_string()),
        ("ai.total_tokens".to_string(), "37".to_string()),
        ("mcp.tool".to_string(), "search".to_string()),
    ]);

    let json = super::telemetry_json(&fields);

    assert!(json.contains("\"ai.asset_id\":\"deepseek-chat\""));
    assert!(json.contains("\"ai.total_tokens\":\"37\""));
    assert!(json.contains("\"mcp.tool\":\"search\""));
}
```

- [ ] **Step 2: Run test to verify it fails**

Run:

```bash
cargo test -p spacegate-kernel telemetry_json_serializes_plugin_defined_fields 2>&1 | head -c 12000
```

Expected: FAIL because `telemetry_json` is missing.

- [ ] **Step 3: Add serde_json dependency if needed**

Check `crates/kernel/Cargo.toml`. If `serde_json` is not already present, add:

```toml
serde_json = { workspace = true }
```

- [ ] **Step 4: Implement telemetry_json**

Add to `crates/kernel/src/observability.rs`:

```rust
pub fn telemetry_json(fields: &BTreeMap<String, String>) -> String {
    serde_json::to_string(fields).unwrap_or_else(|_| "{}".to_string())
}
```

- [ ] **Step 5: Refactor service.rs access log**

In `crates/kernel/src/service.rs`:

1. Replace import of `telemetry_log_attributes` with `telemetry_json`.
2. Delete all hardcoded `telemetry.asset_id`, `telemetry.total_tokens`, `telemetry.mcp_tool`, etc.
3. Emit only:

```rust
let telemetry = telemetry_json(&access_log.telemetry);
tracing::info!(
    event = "http_access",
    gateway = %access_log.gateway,
    method = %access_log.method,
    path = %access_log.path,
    host = %access_log.host,
    protocol_name = %access_log.protocol_name,
    protocol_version = %access_log.protocol_version,
    status_code = access_log.status_code,
    request_id = %access_log.request_id,
    peer_addr = %access_log.peer_addr,
    duration_ms = access_log.duration_ms,
    request_body_size = ?access_log.request_body_size,
    response_body_size = ?access_log.response_body_size,
    telemetry = %telemetry,
    "http access log"
);
```

- [ ] **Step 6: Run tests**

Run:

```bash
cargo test -p spacegate-kernel observability::tests 2>&1 | head -c 12000
```

Expected: PASS.

---

### Task 4: Plugin API Becomes Namespaced and Checked

**Files:**
- Modify: `crates/plugin/src/lib.rs`
- Modify: `crates/plugin/tests/test_telemetry.rs`

- [ ] **Step 1: Update plugin tests**

Replace `crates/plugin/tests/test_telemetry.rs` content with:

```rust
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
```

- [ ] **Step 2: Run tests to verify they fail**

Run:

```bash
cargo test -p spacegate-plugin test_telemetry 2>&1 | head -c 12000
```

Expected: FAIL because `set_telemetry_field` currently returns `()` and `set_plugin_telemetry_field` is missing.

- [ ] **Step 3: Implement plugin APIs**

In `crates/plugin/src/lib.rs`, replace current `set_telemetry_field` with:

```rust
pub fn set_telemetry_field(
    req: &SgRequest,
    key: impl Into<String>,
    value: impl ToString,
) -> Result<(), spacegate_kernel::observability::TelemetryError> {
    if let Some(context) = req.extensions().get::<spacegate_kernel::observability::TelemetryContext>() {
        context.insert_checked(key, value)?;
    }
    Ok(())
}

pub fn set_plugin_telemetry_field(
    req: &SgRequest,
    namespace: &str,
    key: &str,
    value: impl ToString,
) -> Result<(), spacegate_kernel::observability::TelemetryError> {
    if let Some(context) = req.extensions().get::<spacegate_kernel::observability::TelemetryContext>() {
        context.insert_namespaced(namespace, key, value)?;
    }
    Ok(())
}
```

- [ ] **Step 4: Run tests**

Run:

```bash
cargo test -p spacegate-plugin test_telemetry 2>&1 | head -c 12000
```

Expected: PASS.

---

### Task 5: Update ClickHouse Query Script

**Files:**
- Modify: `scripts/otel-local/query-access-logs.sh`

- [ ] **Step 1: Update SQL to parse telemetry JSON**

Replace selected telemetry fields with:

```sql
JSONExtractString(LogAttributes['telemetry'], 'ai.asset_id') AS ai_asset_id,
JSONExtractString(LogAttributes['telemetry'], 'ai.asset_type') AS ai_asset_type,
JSONExtractString(LogAttributes['telemetry'], 'ai.total_tokens') AS ai_total_tokens,
JSONExtractString(LogAttributes['telemetry'], 'mcp.server') AS mcp_server,
JSONExtractString(LogAttributes['telemetry'], 'mcp.tool') AS mcp_tool,
JSONExtractString(LogAttributes['telemetry'], 'mcp.success') AS mcp_success,
JSONExtractString(LogAttributes['telemetry'], 'auth.app_id') AS auth_app_id,
JSONExtractString(LogAttributes['telemetry'], 'auth.api_key_hash') AS auth_api_key_hash
```

Keep base fields:

```sql
Timestamp,
Body,
SeverityText,
LogAttributes['event'] AS event,
LogAttributes['gateway'] AS gateway,
LogAttributes['method'] AS method,
LogAttributes['path'] AS path,
LogAttributes['status_code'] AS status_code,
LogAttributes['request_id'] AS request_id,
LogAttributes['duration_ms'] AS duration_ms,
LogAttributes['telemetry'] AS telemetry
```

- [ ] **Step 2: Validate shell syntax**

Run:

```bash
bash -n scripts/otel-local/query-access-logs.sh
```

Expected: no output and exit code 0.

---

### Task 6: Update WASM Plan

**Files:**
- Modify: `docs/archive/otlp/wasm-telemetry-host-function-plan.md`

- [ ] **Step 1: Align ABI plan with namespaced telemetry**

Update the plan so `spacegate_set_telemetry_field` requires a fully qualified key:

```text
ai.total_tokens
mcp.tool
auth.app_id
```

And add optional convenience SDK wrapper:

```rust
pub fn set_plugin_telemetry_field(namespace: &str, key: &str, value: impl ToString) -> Result<(), Status> {
    set_telemetry_field(&format!("{namespace}.{key}"), value)
}
```

- [ ] **Step 2: Align validation section**

Document the same rules:

- key max 128 bytes
- value max 4096 bytes
- key must contain `.`
- allowed key chars: `[A-Za-z0-9_.-]`
- reserved prefixes rejected: `http.`, `net.`, `gateway.`, `spacegate.`, `otel.`

---

### Task 7: Full Verification

**Files:**
- No code changes.

- [ ] **Step 1: Format touched Rust files**

Run:

```bash
rustfmt --edition 2021 crates/kernel/src/observability.rs crates/kernel/src/service.rs crates/plugin/src/lib.rs crates/plugin/tests/test_telemetry.rs
```

Expected: no output and exit code 0.

- [ ] **Step 2: Run targeted tests**

Run:

```bash
cargo test -p spacegate-kernel observability::tests 2>&1 | head -c 12000
cargo test -p spacegate-plugin test_telemetry 2>&1 | head -c 12000
```

Expected: PASS.

- [ ] **Step 3: Run integration compile check**

Run:

```bash
cargo check -p spacegate-shell --features fs,plugin-wasm 2>&1 | head -c 12000
```

Expected: PASS. Existing unrelated warnings may remain.

- [ ] **Step 4: Validate local scripts**

Run:

```bash
for f in scripts/otel-local/*.sh; do bash -n "$f" || exit 1; done
```

Expected: no output and exit code 0.

---

## Expected Result

插件写：

```rust
set_plugin_telemetry_field(&req, "ai", "asset_id", "deepseek-chat")?;
set_plugin_telemetry_field(&req, "ai", "total_tokens", 37)?;
set_plugin_telemetry_field(&req, "mcp", "tool", "search")?;
```

access log 入库：

```text
LogAttributes['event'] = 'http_access'
LogAttributes['telemetry'] = '{"ai.asset_id":"deepseek-chat","ai.total_tokens":"37","mcp.tool":"search"}'
```

审计查询：

```sql
SELECT
  Timestamp,
  LogAttributes['request_id'] AS request_id,
  JSONExtractString(LogAttributes['telemetry'], 'ai.asset_id') AS asset_id,
  toUInt64OrZero(JSONExtractString(LogAttributes['telemetry'], 'ai.total_tokens')) AS total_tokens,
  JSONExtractString(LogAttributes['telemetry'], 'mcp.tool') AS mcp_tool
FROM otel_logs
WHERE LogAttributes['event'] = 'http_access'
ORDER BY Timestamp DESC;
```

## Self-Review

- Spec coverage: covers namespace, validation, generic JSON access log, plugin API, ClickHouse query, WASM plan alignment.
- Placeholder scan: no TBD/TODO placeholders.
- Type consistency: `TelemetryError` lives in kernel and is returned by plugin APIs; `TelemetryContext` remains request extension.
- Boundary check: no AI/MCP/token business semantics remain in `service.rs`; those appear only in docs/scripts as examples.
