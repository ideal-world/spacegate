# Spacegate MCP Proxy And Plugin Compatibility Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use `superpowers:executing-plans` to implement this plan task-by-task. Keep checkbox state current after every verification step.

**Goal:** Complete MCPRoute as a production proxy for external MCP servers while preserving the full Spacegate/Hai plugin lifecycle, including authentication, JSON body rewriting, streaming SSE inspection, token usage collection and sensitive-output filtering.

**Architecture:** MCPRoute remains a first-class configuration object that compiles into the existing HTTP route runtime. MCP transport handling validates only the HTTP envelope and adds MCP metadata; route and backend plugins retain the existing plugin pipeline. Proxy-Wasm response handling gains an SSE streaming driver, and MCP backend selection gains response-aware session affinity instead of relying only on a session hash.

**Tech Stack:** Rust, Hyper/http-body, Tokio, Tower layers, Wasmtime Proxy-Wasm host, Vue 3, TypeScript, Redis config backend, Kubernetes CRDs.

## Global Constraints

- Do not add any rule that rejects, disables or warns solely because a plugin reads or rewrites an MCP body.
- Authentication and route plugins must execute before a request reaches the external MCP server.
- MCP transport code must not parse JSON-RPC tool/resource/prompt semantics.
- Ordinary JSON responses may continue using bounded full-body processing; `text/event-stream` responses must remain incremental.
- Preserve existing HTTPRoute, native plugin and non-MCP Wasm behavior.
- Do not introduce a Redis dependency into `spacegate-kernel`.
- Do not automatically create Git commits; the user controls commit timing.

---

## Completion Checklist

- [ ] MCPRoute route plugins authenticate and short-circuit requests before upstream.
- [ ] MCPRoute preserves plugin priority and request/response ordering.
- [ ] Hai Wasm plugins receive incremental SSE response-body callbacks.
- [ ] Hai token usage and sensitive-output handling work on MCP SSE chunks.
- [ ] Ordinary JSON response replacement remains complete-body compatible.
- [ ] Client disconnect cleans up Proxy-Wasm HTTP context and releases VM capacity.
- [ ] Session requests return to the backend that created the session.
- [ ] Session termination requests can reach a Streamable HTTP upstream.
- [ ] Legacy SSE clients are not rejected for an unnecessary `Accept` header.
- [ ] MCPRoute Wasm binding IDs are unique per route and backend.
- [ ] File, Redis, Admin API and K8S round trips preserve MCPRoute plugin bindings.
- [ ] Documentation no longer recommends blocking body-processing plugins.

---

### Task 1: Lock The MCP Plugin Pipeline Contract

**Files:**
- Modify: `crates/kernel/src/helper_layers/mcp_proxy.rs`
- Modify: `crates/shell/src/server.rs`
- Modify: `docs/mcp/mcp-route-guide.md`

**Interfaces:**
- Consumes: `SgMcpRoute.plugins`, `McpProxyLayer`, `fold_layers`.
- Produces: regression tests proving MCP transport handling does not bypass or reject route plugins.

- [x] Add a shell test named `mcp_route_preserves_plugin_bindings_and_priority` that builds an `SgMcpRoute` with two `PluginBinding` values and asserts the compiled `SgHttpRoute.plugins` retains both IDs and priorities.
- [x] Add a kernel service test named `mcp_auth_layer_can_short_circuit_before_transport_and_upstream` using an outer test layer that returns `401`; assert neither `McpProxyLayer`'s inner service nor the mock upstream is called.
- [x] Add a kernel service test named `mcp_outer_plugin_can_replace_json_body` that modifies a finite JSON response after `McpProxyLayer` returns it.
- [x] Run `cargo test -p spacegate-kernel mcp_proxy` and confirm all MCP layer tests pass.
- [x] Run `cargo test -p spacegate-shell mcp_route` and confirm MCP compilation tests pass.
- [x] Remove the guide text that recommends rejecting or warning on body-inspection plugins; state that plugin behavior is preserved and that the host must stream SSE callbacks.

### Task 2: Add A Reproducible Proxy-Wasm SSE Guest

**Files:**
- Create: `crates/plugin-wasm/tests/streaming_body_guest/Cargo.toml`
- Create: `crates/plugin-wasm/tests/streaming_body_guest/src/lib.rs`
- Create: `crates/plugin-wasm/tests/streaming_body.rs`
- Modify: `crates/plugin-wasm/Cargo.toml` only if the test target requires an already-workspace-managed dependency.

**Interfaces:**
- Consumes: Proxy-Wasm ABI `proxy_on_response_headers`, `proxy_on_response_body`, `proxy_get_buffer_bytes`, `proxy_set_buffer_bytes`.
- Produces: a deterministic guest that records each body callback and rewrites each SSE `data:` chunk.

- [x] Implement the test guest so `proxy_on_response_body` prefixes each received SSE payload with `filtered:` and records callback count plus `end_of_stream` in shared data.
- [x] Add a mock upstream body that emits `data: first\n\n`, waits on a Tokio notification, then emits `data: second\n\n` and EOF.
- [x] Write `wasm_response_body_hook_receives_first_sse_chunk_before_upstream_eof`; require the first rewritten chunk before releasing the second chunk.
- [x] Write `wasm_response_body_hook_receives_eof_once`; assert the guest sees one terminal callback.
- [x] Run `cargo test -p spacegate-plugin-wasm --test streaming_body` and confirm the first test fails because `Vm::process` waits in `collect_body_limited`.

### Task 3: Stream SSE Through The Proxy-Wasm Host

**Files:**
- Create: `crates/plugin-wasm/src/streaming_body.rs`
- Modify: `crates/plugin-wasm/src/lib.rs`
- Modify: `crates/plugin-wasm/src/vm.rs`
- Modify: `crates/plugin-wasm/src/shell.rs`
- Modify: `crates/plugin-wasm/src/host_state.rs` if lifecycle state needs an explicit streaming phase.

**Interfaces:**
- Produces: `PreparedWasmResponseStream`, carrying response parts, upstream `SgBody` and `http_context_id`.
- Produces: `Vm::process_response_chunk(http_context_id, bytes, end_of_stream)`, returning the plugin-modified bytes and action.
- Produces: `Vm::finish_response_stream(http_context_id)`, driving trailers/log/done/delete exactly once.

- [x] Change `Vm::process` so a response with `Content-Type: text/event-stream` and an exported response-body hook returns a prepared stream instead of collecting the complete body.
- [x] Keep the existing bounded full-body path for non-SSE responses so Hai's end-of-stream JSON replacement behavior remains intact.
- [x] Add a channel-backed `SgBody` worker that polls upstream frames, locks the selected VM only while invoking a callback, forwards modified bytes immediately and releases the lock between chunks.
- [x] Set the active Proxy-Wasm context before every chunk callback so buffer APIs address the correct request when multiple streams share a VM.
- [x] Pass `end_of_stream=false` for data chunks and issue one `end_of_stream=true` callback when upstream finishes.
- [x] Preserve upstream trailers and invoke response-trailer hooks before lifecycle completion.
- [x] On client cancellation, schedule context cleanup and release the streaming inflight guard.
- [x] Do not hold `MutexGuard<Vm>` for the full SSE connection lifetime.
- [x] Run `cargo test -p spacegate-plugin-wasm --test streaming_body` and confirm all streaming tests pass.
- [x] Run `cargo test -p spacegate-plugin-wasm` and confirm existing finite-body, dispatch, tick and SDK compatibility tests remain green.

### Task 4: Verify Hai Streaming Semantics

**Files:**
- Modify: `crates/plugin-wasm/tests/streaming_body.rs`
- Modify: `docs/mcp/mcp-route-guide.md`
- Reference only: `hai-process-mix/plugins/mix/src/http_filter.rs`

**Interfaces:**
- Consumes: Hai behavior in `on_http_response_body`: per-chunk usage parsing, streaming output guard and EOF usage reporting.
- Produces: host-level tests matching the callback semantics Hai already implements.

- [x] Add a test whose guest accumulates usage from two SSE chunks and publishes the final total only on EOF.
- [x] Add a test whose guest removes a blocked SSE chunk and allows later cleanup to complete.
- [x] Add a concurrent-stream test with `vm_pool_size=1`; prove the second request can make progress between chunks of the first stream.
- [x] Add a disconnect test; drop the client body before EOF and assert a subsequent request can reuse the VM slot.
- [x] Document that Spacegate supports Hai streaming hooks without requiring a special MCP plugin capability declaration.

### Task 5: Replace Hash-Only MCP Affinity With Response-Aware Affinity

**Files:**
- Create: `crates/kernel/src/helper_layers/balancer/mcp_session_affinity.rs`
- Modify: `crates/kernel/src/helper_layers/balancer.rs`
- Modify: `crates/kernel/src/service/http_route.rs`
- Modify: `crates/shell/src/server.rs`

**Interfaces:**
- Produces: `McpSessionAffinityBalancer<S>` with bounded, TTL-controlled `session_id -> backend_index` state.
- Consumes: request and response `Mcp-Session-Id` headers.
- Preserves: IP hash for an initialization request without a session header.

- [ ] Write a failing test: initialization without a session selects backend A, backend A returns `Mcp-Session-Id: session-1`, and the next request with `session-1` must select A even when hashing `session-1` would select B.
- [ ] Write tests for same-session stability, different sessions, one backend, missing peer IP and unknown sessions.
- [ ] Implement backend selection by index so the selected backend is available while inspecting the response future.
- [ ] Record the response session header against the selected backend before returning the response downstream.
- [ ] Bound the map by capacity and TTL; expired or missing entries fall back to session hash, then IP hash when no session exists.
- [ ] Remove the mapping after a successful session-termination request.
- [ ] Ensure route reload creates a fresh map and does not retain indices from a replaced backend list.
- [ ] Run `cargo test -p spacegate-kernel mcp_session` and confirm all session tests pass.

### Task 6: Define The Multi-Pod Session Boundary

**Files:**
- Modify: `docs/mcp/mcp-route-guide.md`
- Modify: `resource/kube-manifests/spacegate-mcproute.yaml` only if a new public option is approved during implementation review.

**Interfaces:**
- Consumes: process-local response-aware affinity from Task 5.
- Produces: an explicit deployment contract without coupling kernel to Redis.

- [ ] Document that the in-memory mapping is correct for one Spacegate process or when ingress keeps a client on one Spacegate Pod.
- [ ] Document that multi-Pod Spacegate without ingress stickiness requires either shared upstream MCP sessions or a shared session-store extension.
- [ ] Do not silently reuse configuration-storage Redis for runtime MCP sessions.
- [ ] Treat a distributed Redis session store as a separate public-interface change requiring model, K8S CRD, SDK, frontend and failure-policy design before implementation.

### Task 7: Complete MCP Transport Method Compatibility

**Files:**
- Modify: `crates/kernel/src/helper_layers/mcp_proxy.rs`
- Modify: `crates/shell/src/server.rs`
- Modify: `crates/model/src/http_route.rs` only if method policy becomes configurable.

**Interfaces:**
- Consumes: Streamable HTTP path and Legacy SSE paths.
- Produces: valid method forwarding without weakening plugin authentication.

- [ ] Add Streamable HTTP `DELETE` to the compiled MCP path matches.
- [ ] Update `McpProxyLayer` to allow `DELETE` with no body and preserve `Mcp-Session-Id`.
- [ ] Add a test proving the upstream receives DELETE and the affinity mapping is removed after a successful response.
- [ ] Change Legacy SSE message POST validation to require JSON `Content-Type` without requiring `Accept: application/json`.
- [ ] Keep Streamable HTTP POST validation requiring JSON `Content-Type` and both advertised response media types.
- [ ] Add tests for 405, 406 and 415 responses so valid compatibility changes do not weaken invalid-request handling.

### Task 8: Make Wasm Binding IDs Route-Specific

**Files:**
- Modify: `../spacegate-admin-fe/components/config/src/components/McpRouteForm.vue`
- Modify: `../spacegate-admin-fe/components/config/src/components/BackendForm.vue`
- Modify: `../spacegate-admin-fe/components/config/src/views/Routes.vue`
- Modify: `../spacegate-admin-fe/components/config/src/utils/wasmPlugin.test.ts`

**Interfaces:**
- Consumes: actual route name and backend index.
- Produces: stable binding owners such as `mcp-route/<route-name>` and `mcp-route/<route-name>/backend/<index>`.

- [ ] Replace the constant MCP `binding-name="mcp-route"` with a value derived from the actual route name.
- [ ] Pass an explicit backend binding owner from `McpRouteForm` into each `BackendForm`.
- [ ] Add tests proving two MCP routes binding the same Hai plugin generate different instance IDs.
- [ ] Add tests proving editing one route preserves its existing binding ID and does not update the other route's runtime configuration.
- [ ] Run the component TypeScript tests with Node 24.
- [ ] Run `npm run build` in `spacegate-admin-fe/components/config` with Node 24.

### Task 9: Add MCP End-To-End Runtime Tests

**Files:**
- Create: `crates/shell/tests/mcp_proxy.rs`
- Modify: `crates/shell/Cargo.toml` only for existing workspace test utilities.

**Interfaces:**
- Consumes: compiled MCPRoute, mock plugins, mock MCP upstreams and response-aware affinity.
- Produces: executable acceptance coverage for the complete runtime path.

- [ ] Test missing credentials: authentication plugin returns 401 and upstream call count stays zero.
- [ ] Test valid credentials: POST JSON reaches upstream and response JSON can be rewritten by a plugin.
- [ ] Test GET SSE: the first chunk reaches the client before the second chunk is produced.
- [ ] Test POST SSE: headers and body chunks remain available to the plugin chain.
- [ ] Test Legacy SSE `/sse` and `/message` forwarding.
- [ ] Test `Authorization`, `MCP-Protocol-Version`, `Mcp-Session-Id` and `Last-Event-ID` request-header forwarding.
- [ ] Test response `Mcp-Session-Id` forwarding and backend association.
- [ ] Test disabled timeout with a short test threshold rather than waiting 30 seconds.
- [ ] Test plugin priorities on request and reverse response traversal.
- [ ] Run `cargo test -p spacegate-shell --test mcp_proxy` and confirm every case passes.

### Task 10: Verify Persistence, Hot Reload And Deployment Paths

**Files:**
- Modify: `crates/model/tests/test_parse_config.rs`
- Modify: `crates/config/tests/test_k8s_config.rs`
- Create or modify: focused Redis route round-trip test under `crates/config/tests/`
- Modify: `crates/model/tests/k8s_deployment_assets.rs`
- Modify: `binary/admin-server/src/service/config.rs`
- Verify: `sdk/admin-client/src/api/index.ts`
- Verify: `sdk/admin-client/src/model/SgRoute.ts`
- Verify: `sdk/admin-client/src/model/SgMcpRoute.ts`

**Interfaces:**
- Consumes: `SgRoute::Mcp`, `PluginBinding`, K8S `MCPRoute` plus `SgFilter`, Redis route serialization.
- Produces: round-trip and reload proof for all supported configuration sources.

- [ ] Add file-config round-trip coverage preserving MCP route plugins, backend plugins and priorities.
- [ ] Add Redis create/retrieve/update/delete coverage for the same fields using the existing Redis testcontainer pattern.
- [ ] Add K8S conversion coverage for MCPRoute route plugins and backend Wasm plugins.
- [ ] Verify watch/update/delete events rebuild the MCP router without losing plugin bindings.
- [ ] Add an Admin API route CRUD test that creates an MCPRoute with route and backend plugin bindings, reads it back, updates it and confirms no MCP or plugin field is lost.
- [ ] Verify the generated SDK route CRUD methods accept and return `SgRoute`, and its union still exposes all `SgMcpRoute` fields.
- [ ] Keep kind/k3d installation, watch, update and deletion as an explicit environment test command in the guide.
- [ ] Run `cargo test -p spacegate-model` and `cargo test -p spacegate-config` with required K8S features.
- [ ] Run `cargo test -p spacegate-admin-server` and the SDK TypeScript type check.

### Task 11: Final Verification And Documentation

**Files:**
- Modify: `docs/mcp/mcp-route-guide.md`
- Modify: `README.md` only if the user-facing local test entrypoint belongs there.

- [ ] Document the exact runtime order: gateway plugins, route plugins, MCP envelope, backend plugins, upstream.
- [ ] Document finite JSON versus SSE Proxy-Wasm body behavior.
- [ ] Document true session mapping and its single-process/multi-Pod boundary.
- [ ] Add curl examples for Streamable HTTP POST, SSE GET, DELETE and Legacy SSE.
- [ ] Run `cargo fmt --check` for all changed Rust crates.
- [ ] Run focused tests from Tasks 1 through 10.
- [ ] Run `git diff --check` in `spacegate` and the frontend component repository.
- [ ] Record any environment-only tests that could not run; do not report them as passed.

---

## New Session Handoff

Start the dedicated MCP session with this request:

```text
请按照 spacegate/docs/superpowers/plans/2026-07-26-mcp-proxy-plugin-compatibility.md 执行 MCP 专项修复。

边界：
1. MCPRoute 必须继续支持鉴权、body 解析、body 改写、敏感词过滤和 token 用量统计插件，不能新增禁止 body 插件的限制。
2. 优先修复 Proxy-Wasm SSE 逐 chunk 回调，再修复 response-aware session affinity。
3. 每个任务先写失败测试，再做最小实现；完成后更新计划复选框。
4. 保持现有 HTTPRoute、非 MCP Wasm 和 K8S 配置兼容。
5. 不自动提交 Git commit。
```
