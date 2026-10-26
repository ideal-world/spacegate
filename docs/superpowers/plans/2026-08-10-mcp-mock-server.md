# MCP Dual-Protocol Mock Server Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use `superpowers:executing-plans` to implement this plan task-by-task. Keep checkbox state current after every verification step.

**Goal:** Add a local, stateful mock MCP upstream that exposes Streamable HTTP and Legacy SSE for Spacegate integration testing.

**Architecture:** A standalone example binary owns an in-memory session registry. Hyper dispatches the five protocol endpoints; a Legacy SSE connection keeps an `mpsc` receiver while POST sends a JSON-RPC response into the stored sender.

**Tech Stack:** Rust, Tokio, Hyper, http-body-util, futures-util, serde_json.

## Global Constraints

- Reuse workspace dependencies; do not add an MCP SDK or a new crate dependency.
- Do not modify Spacegate gateway configuration or runtime code.
- The mock must distinguish Streamable HTTP sessions from Legacy SSE connections.
- Legacy POST must send the result through the existing SSE connection, not its HTTP response body.
- Do not automatically create Git commits; the user controls commit timing.

---

### Task 1: Add the failing Legacy SSE lifecycle test

**Files:**
- Create: `examples/mcp-mock-server/src/lib.rs`
- Create: `examples/mcp-mock-server/src/lib_tests.rs`

**Interfaces:**
- Produces: `MockState::open_legacy_session`, `MockState::send_legacy_response`, and `MockState::remove_session`.

- [ ] Write `legacy_message_is_delivered_to_the_original_sse_connection`: open a session, assert the endpoint event is available, send a `tools/list` request, and assert the receiver gets its JSON-RPC result.
- [ ] Run `cargo test -p mcp-mock-server legacy_message_is_delivered_to_the_original_sse_connection`; expect compilation failure because the package does not exist.
- [ ] Implement the smallest session registry using `Arc<Mutex<HashMap<...>>>` and a Tokio `mpsc` channel.
- [ ] Re-run the test; expect pass.

### Task 2: Expose both MCP transports

**Files:**
- Create: `examples/mcp-mock-server/Cargo.toml`
- Create: `examples/mcp-mock-server/src/main.rs`
- Modify: `Cargo.toml`

**Interfaces:**
- Produces: `handle_request(Request<Incoming>, MockState) -> Response<MockBody>`.
- Consumes: `MockState` from Task 1.

- [ ] Write failing handler tests for Streamable `initialize`, `tools/list`, `tools/call` SSE, `GET`, `DELETE`, Legacy endpoint response, and unknown Legacy session `404`.
- [ ] Run `cargo test -p mcp-mock-server`; expect failures before handler implementation.
- [ ] Implement only `/mcp`, `/sse`, and `/message`; reject unsupported paths and methods with JSON `404`/`405` responses.
- [ ] Register `examples/mcp-mock-server` as a workspace member and add a Tokio TCP listener main function with `--addr` defaulting to `127.0.0.1:3001`.
- [ ] Re-run `cargo test -p mcp-mock-server`; expect all tests pass.

### Task 3: Document standalone and gateway-facing checks

**Files:**
- Create: `examples/mcp-mock-server/README.md`

**Interfaces:**
- Consumes: the binary endpoint contract from Task 2.
- Produces: startup, direct curl checks, and MCPRoute backend configuration snippets.

- [ ] Document `cargo run -p mcp-mock-server -- --addr 127.0.0.1:3001`.
- [ ] Document direct Streamable HTTP initialize, tools/list, SSE and DELETE checks.
- [ ] Document the Legacy SSE long-connection sequence and a shell command to send message requests using the received endpoint URL.
- [ ] Include separate Streamable HTTP and Legacy SSE MCPRoute backend snippets without asking the user to apply them.
- [ ] Run `cargo fmt --check` and `cargo test -p mcp-mock-server`.
