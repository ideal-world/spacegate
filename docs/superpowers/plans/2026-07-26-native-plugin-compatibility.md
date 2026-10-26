# Spacegate Native Plugin Compatibility Implementation Plan

> **For agentic workers:** Execute one task at a time, keep the checkbox state current after verification, and do not create Git commits automatically.

**Goal:** Preserve dynamic plugin distribution while making the compatibility boundary explicit: official native plugins must fail safely when incompatible, third-party plugins should be portable through Wasm, and a future native SDK must use a versioned stable ABI rather than Rust internal types.

**Architecture:** HAI official native plugins are statically linked into the paired gateway binary, so their async runtime state never crosses a Rust dylib boundary. Keep the existing `extern "Rust"` dylib loader only as a legacy external-plugin path during migration. Portable custom plugins use the existing Wasm host. If a native third-party SDK is still required after that, add an opt-in `cdylib` C ABI v1 with opaque host-owned request/response handles and version negotiation.

**Tech Stack:** Rust, Cargo, Docker BuildKit, Proxy-Wasm/Wasmtime, Hyper, C ABI, Kubernetes image rollout.

## Global Constraints

- Do not pass `PluginRepository`, Rust trait objects, `Future`, `String`, `Vec`, `hyper::Request`, `hyper::Response`, or `SgBody` across any new native-plugin ABI.
- Do not remove the existing dylib loader until all official native plugins have a supported migration path.
- Keep the current file, OCI and HTTP(S) Wasm distribution paths working.
- Keep native plugin discovery, configuration instances, route bindings and mounting as separate concerns.
- A failed compatibility check must reject the plugin with an actionable log message; it must never crash the gateway.
- Do not add an implicit Redis dependency to `spacegate-kernel`.
- Do not overlap MCP streaming work already tracked in `2026-07-26-mcp-proxy-plugin-compatibility.md`.

---

## Compatibility Model

| Plugin category | Distribution | Execution contract | Compatibility expectation |
| --- | --- | --- | --- |
| Official HAI native plugin | Statically linked paired gateway binary | One process and one Tokio runtime | Built and released with the exact gateway dependency graph |
| Legacy external native plugin | Image-provided or mounted `.so` | Existing Rust dylib contract during migration | Metadata-checked; not a portable public ABI |
| Custom portable plugin | File, OCI image or HTTP(S) Wasm | Existing Proxy-Wasm host ABI | Independent of host Rust crate versions |
| Future custom native plugin | `.so` / OCI native artifact | New versioned C ABI | Independent Rust compiler and Cargo lockfile; still bound to OS, CPU and libc target |

The existing `extern "Rust"` loader supports runtime code discovery but is not a stable public ABI. It remains usable for official paired artifacts only. A stable native extension point requires the new ABI described in Phase 3.

---

## Completion Checklist

- [x] all-in-one and K8s images build the static HAI gateway binary from the paired gateway dependency graph.
- [x] regression test proves a missing `Hai-Api-Key` produces repeated 401 responses without terminating the gateway.
- [ ] every legacy native plugin declares its intended gateway compatibility range and target platform.
- [ ] the legacy loader rejects an incompatible native plugin before registration.
- [ ] portable custom-plugin documentation directs new user plugins to Wasm and lists supported host capabilities.
- [ ] native ABI v1 has a reviewed public header, SDK crate and conformance test suite.
- [ ] a v1 plugin built by an independent sample project can load into a compatible gateway and safely be rejected by an incompatible one.
- [ ] no new native plugin API leaks Rust-owned HTTP or async types over the binary boundary.

---

### Task 1: Statically Link The Official HAI Plugin Set

**Files:**

- Modify: `Dockerfile.all-in-one`
- Modify: `resource/docker/spacegate-k8s/Dockerfile`
- Test: `docker/all-in-one/tests/test-hai-plugin-build.sh`

**Purpose:** Eliminate the unsafe async Rust dylib boundary for the official HAI plugin set while preserving the generic legacy loader for separately mounted plugins.

- [x] Build `hai-hub-spacegate`, which registers the HAI plugins before constructing the process Tokio runtime.
- [x] Replace the in-image HAI `.so` copy with the static HAI gateway binary in all-in-one and K8s images.
- [x] Keep all-in-one external legacy dylib discovery through `PLUGIN_DIR`; keep the K8s `/lib/spacegate/plugins` discovery directory.
- [x] Add a shell regression test that rejects a bundled HAI `.so` and requires the static binary/startup contract in both images.
- [x] Build both HAI builder stages and a full all-in-one image; issue two unauthenticated HAI requests and one Redis-backed request without process exit.

**Acceptance:** The official HAI plugin set executes in the same Rust process and Tokio runtime as its gateway; legacy external dylibs remain separate and are not treated as a portable public ABI.

### Task 2: Add Legacy Native Plugin Compatibility Metadata And Load-Time Rejection

**Files:**

- Create: `crates/plugin/src/dynamic/metadata.rs`
- Modify: `crates/plugin/src/dynamic.rs`
- Modify: `crates/plugin/Cargo.toml`
- Create: `crates/plugin/tests/dynamic_metadata.rs`
- Create: `resource/plugin-sdk/native-plugin.toml.example`
- Modify: `docs/develop/DEVELOPMENT.md`

**Purpose:** Turn an implicit binary compatibility assumption into an explicit, checked contract without changing the current plugin execution API.

**Public metadata contract:**

```toml
plugin_api = "legacy-rust-v1"
gateway_version = "0.2.0-alpha.4"
target = "x86_64-unknown-linux-gnu"
rustc = "1.88"
```

- [ ] Write a failing test that loads a fixture with `plugin_api = "legacy-rust-v1"` and a mismatched `target`; expect a structured compatibility error before `register` is looked up.
- [ ] Write a failing test that accepts an exact metadata match and invokes the fixture registration function.
- [ ] Add `LegacyNativePluginMetadata` with serde parsing and a `validate_against_host()` method. The method compares ABI generation, gateway version policy and target triple; it does not compare arbitrary Cargo package versions at runtime.
- [ ] Make `register_dylib()` load `<library>.toml` before `libloading::Library::new()`. Missing metadata remains allowed only for image-provided official plugins during the migration window; external native plugin directories require metadata.
- [ ] Log the rejected file path, expected and received ABI generation, host target and remediation command. Do not panic and do not call `register` after rejection.
- [ ] Document that this metadata prevents accidental loading of known-incompatible artifacts but cannot make the legacy Rust ABI portable.
- [ ] Run `cargo test -p spacegate-plugin dynamic_metadata` and `cargo test -p spacegate-plugin`.

**Acceptance:** An external incompatible legacy `.so` is rejected before registration with a deterministic error, never a gateway crash.

### Task 3: Document And Verify The Existing Wasm Custom Plugin Path

**Files:**

- Modify: `docs/develop/DEVELOPMENT.md`
- Modify: `README.md`
- Create: `docs/develop/wasm-plugin-capability-matrix.md`
- Test: existing focused tests under `crates/plugin-wasm/tests/`

**Purpose:** Make the already-supported Wasm path discoverable and contractually clear for third-party developers. This task does not change the Wasm runtime, fetcher or VM lifecycle.

- [ ] Inventory current Proxy-Wasm host capabilities from source and existing tests: request headers/body, response headers/body, SSE response callbacks, upstream HTTP call, shared data, logs and telemetry fields.
- [ ] Publish the matrix with each capability marked supported, bounded-body only, SSE streaming supported, or unavailable. Link every supported claim to an existing host test.
- [ ] Document file, OCI and HTTP(S) loading, checksum verification, HTTP headers for signed downloads, and cache behavior.
- [ ] Document the required target (`wasm32-wasip1`), supported plugin schema file (`/schema.json`) and local test commands.
- [ ] State that new user-developed plugins should use Wasm unless they have an approved native ABI v1 requirement.
- [ ] Run the existing fetch, HTTP-call, SDK-example and streaming-body tests before documenting their corresponding capabilities.
- [ ] If the inventory finds a required but unsupported capability, create a separate approved implementation task; do not add Wasm runtime changes as part of this documentation task.
- [ ] Run `cargo test -p spacegate-plugin-wasm` and the affected admin frontend type/build checks.

**Acceptance:** A third-party developer can choose the existing Wasm host with a clear capability and packaging contract, without matching the gateway Rust lockfile; no Wasm runtime behavior changes in this task.

### Task 4: Design And Approve Native Plugin ABI V1

**Files:**

- Create: `docs/develop/native-plugin-abi-v1.md`
- Create: `crates/plugin-native-sdk/include/spacegate_plugin_v1.h`
- Create: `crates/plugin-native-sdk/README.md`
- Create: `crates/plugin-native-sdk/Cargo.toml`
- Create: `crates/plugin-native-sdk/src/lib.rs`

**Purpose:** Define the public interface before any loader implementation. This task is a design and compile-only SDK milestone; it does not switch production loading.

**Required ABI shape:**

```c
typedef struct sg_plugin_api_v1 {
  uint32_t abi_version;
  const char *plugin_code;
  int32_t (*on_request)(sg_request_handle request, const sg_host_api_v1 *host);
  int32_t (*on_response)(sg_response_handle response, const sg_host_api_v1 *host);
} sg_plugin_api_v1;

const sg_plugin_api_v1 *sg_plugin_entry_v1(void);
```

- [ ] Specify ownership for every pointer and byte slice: host owns request/response handles; plugins borrow data only during a callback; plugin-allocated memory is released by a plugin-provided release callback.
- [ ] Specify host callbacks for headers, body chunks, response status/headers/body, logging and telemetry. All callbacks return fixed integer error codes.
- [ ] Specify async behavior using host-driven continuation callbacks or poll handles; do not export Rust futures.
- [ ] Define `abi_version`, capability bit flags, minimum gateway version and a feature-negotiation response.
- [ ] Define cancellation, timeout and panic boundaries. A native plugin panic or invalid return code must become a controlled 500/error event, not unwind through FFI.
- [ ] Generate a Rust wrapper that exposes ergonomic borrowed request/response views but compiles to the C ABI header only.
- [ ] Add `cbindgen` only if the generated header cannot be maintained from the small checked-in C definition; otherwise keep dependencies unchanged.
- [ ] Add compile tests for the Rust SDK and a minimal C fixture that exports `sg_plugin_entry_v1`.
- [ ] Hold an implementation review before proceeding to Task 5 because this is a new public plugin API.

**Acceptance:** ABI v1 is a reviewed, language-neutral contract with no Rust-owned HTTP, body or asynchronous types crossing the boundary.

### Task 5: Implement A Parallel Native ABI V1 Loader

**Files:**

- Create: `crates/plugin/src/native_abi/mod.rs`
- Create: `crates/plugin/src/native_abi/loader.rs`
- Create: `crates/plugin/src/native_abi/host.rs`
- Modify: `crates/plugin/src/lib.rs`
- Modify: `crates/plugin/Cargo.toml`
- Create: `crates/plugin/tests/native_abi_v1.rs`

**Purpose:** Load ABI v1 plugins beside, not in place of, legacy dylibs.

- [ ] Write a failing test that loads the C fixture, verifies `abi_version == 1`, registers its declared code and receives a header-only response mutation.
- [ ] Write a failing test that loads a fixture declaring ABI v2 and asserts the gateway rejects it without calling the entrypoint callbacks.
- [ ] Write a failing test that causes the fixture to return an invalid status and asserts the request receives a controlled error while the gateway accepts a later request.
- [ ] Implement symbol lookup for `sg_plugin_entry_v1` with `extern "C"`; verify the descriptor before allocating any plugin instance.
- [ ] Implement opaque request and response handle tables owned by the host. Invalidate handles after each callback and reject stale-handle access.
- [ ] Implement the first capability slice only: request headers, response status/header mutation, logging and telemetry. Keep body streaming disabled until Task 6.
- [ ] Keep loaded libraries alive for the process lifetime, matching current native lifecycle behavior; unloading remains out of scope.
- [ ] Run `cargo test -p spacegate-plugin native_abi_v1` and the complete `cargo test -p spacegate-plugin` suite.

**Acceptance:** A compatible independent native fixture can load and mutate headers; incompatible or faulty fixtures fail safely without process termination.

### Task 6: Add Native ABI V1 Body And Streaming Capabilities

**Files:**

- Modify: `crates/plugin/src/native_abi/host.rs`
- Modify: `crates/plugin/src/native_abi/loader.rs`
- Modify: `crates/plugin/tests/native_abi_v1.rs`
- Modify: `docs/develop/native-plugin-abi-v1.md`

**Purpose:** Support the HAI-class body use cases without exposing `SgBody` across FFI.

- [ ] Write a failing test where an ABI v1 fixture receives a finite JSON response body, replaces it, and the client receives the replacement.
- [ ] Write a failing SSE test where a fixture receives the first chunk before upstream EOF and forwards a modified first chunk without buffering the stream.
- [ ] Write a cancellation test proving a disconnected client invalidates handles and a subsequent request still executes.
- [ ] Add host-managed byte-slice callbacks for bounded JSON bodies and stream chunk callbacks for `text/event-stream`.
- [ ] Make response replacement allocate through the host allocator or an explicit plugin release callback; never free plugin memory through a Rust allocator on the host side.
- [ ] Preserve the existing route plugin priority and reverse response traversal semantics.
- [ ] Run the focused native ABI tests together with `cargo test -p spacegate-plugin-wasm` to ensure native and Wasm streaming behavior stays independent.

**Acceptance:** Native ABI v1 can support response filtering and usage collection without raw `SgBody` or Rust futures crossing the ABI boundary.

### Task 7: Define Distribution, Deployment And Migration Policy

**Files:**

- Modify: `Dockerfile.all-in-one`
- Modify: `resource/docker/spacegate-k8s/Dockerfile`
- Modify: `README.md`
- Modify: `docs/k8s/development.md`
- Modify: `docs/develop/DEVELOPMENT.md`

**Purpose:** Make deployment behavior explicit after ABI v1 is proven.

- [ ] Keep HAI plugins on the legacy official-paired path until an ABI v1 or Wasm migration passes the same auth/Redis/response tests.
- [ ] Add an image build check that records the native ABI generation and target triple next to each built-in plugin artifact.
- [ ] Document that new native ABI v1 plugins can be mounted from the external native plugin directory, but discovery still occurs at gateway startup.
- [ ] Document that adding a new native library requires restart/rollout; plugin instance configuration updates remain a separate hot-update operation.
- [ ] Document K8s rollout, all-in-one local mount and OCI distribution procedures for Wasm and native ABI v1 artifacts.
- [ ] Add an all-in-one smoke test: an ABI v1 sample mounts from the external directory, passes compatibility validation and serves a request.
- [ ] Add a K8s image smoke-test command or manifest that validates the same artifact and documents the required target platform.

**Acceptance:** Operators know which plugin classes are portable, which are official-paired, and when restart or rollout is required.

---

## Execution Order

1. Task 1 is complete and remains a release gate.
2. Execute Task 2 before permitting external legacy `.so` files in production.
3. Execute Task 3 as documentation and verification only; reuse MCP streaming tests rather than duplicating or changing them.
4. Stop after Task 4 for API review. Do not begin the public ABI implementation without explicit approval of the header and ownership rules.
5. Implement Tasks 5 and 6 only after the ABI review; then execute Task 7 as the deployment handoff.

## Final Verification

- Run `sh docker/all-in-one/tests/test-hai-plugin-build.sh`.
- Build all-in-one and K8s images using their documented external `hai_hub` build contexts.
- Verify a legacy HAI missing-key request returns repeated 401 responses without process exit.
- Verify a valid HAI request still reaches Redis-backed authentication and the configured upstream/asset chain.
- Verify a rejected legacy native plugin and an incompatible ABI v1 fixture leave the gateway process running.
- Run every focused Rust test introduced by Tasks 2, 5 and 6, followed by `cargo test -p spacegate-plugin`.
- Run `git diff --check` in the `spacegate` repository and record any environment-only tests that could not run.
