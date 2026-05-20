# Spacegate Wasm Plugins

This directory is the dedicated development workspace for Spacegate Proxy-Wasm plugins.

## Layout

```text
plugins/wasm/
  Cargo.toml
  hello-world/
    Cargo.toml
    src/lib.rs
    plugin.yaml
  ai-gateway-queue/
    Cargo.toml
    src/lib.rs
    plugin.yaml
```

Use this directory for plugin source code. Keep compiled `.wasm` files in `resource/wasm/` for local demos, or publish them as OCI artifacts/images for Kubernetes usage.

## Build

Install the wasm target once:

```bash
rustup target add wasm32-wasip1
```

Build all plugins:

```bash
cargo build --release --target wasm32-wasip1 --manifest-path plugins/wasm/Cargo.toml
```

The output for `hello-world` is:

```text
plugins/wasm/target/wasm32-wasip1/release/spacegate_plugin_hello_world.wasm
```

The AI gateway queue plugin output is:

```text
plugins/wasm/target/wasm32-wasip1/release/spacegate_plugin_ai_gateway_queue.wasm
```

If you run commands from inside `plugins/wasm/`, the local `.cargo/config.toml` already sets the wasm target:

```bash
cd plugins/wasm
cargo build --release
```

For a local file-based demo, copy or package the built wasm into `resource/wasm/` and reference it with `file://...`.

For production-style delivery, publish the wasm as an OCI artifact/image and reference it from a Higress-compatible `WasmPlugin`:

```yaml
spec:
  url: oci://registry.example.com/spacegate/plugins/hello-world:v1
```

## Adding A Plugin

1. Create `plugins/wasm/<plugin-name>/`.
2. Add it to `plugins/wasm/Cargo.toml` under `workspace.members`.
3. Set the crate type to `cdylib`.
4. Implement the Proxy-Wasm entry point with `proxy_wasm::main!`.
5. Add a `plugin.yaml` example that shows the intended `WasmPlugin` config.
