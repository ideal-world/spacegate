# Spacegate Wasm Hello World

This is a minimal Proxy-Wasm guest plugin for Spacegate.

Build the wasm:

```bash
cd examples/wasm-hello
cargo build --release --target wasm32-wasip1
cd ../..
cp examples/wasm-hello/target/wasm32-wasip1/release/spacegate_wasm_hello.wasm resource/wasm/spacegate_wasm_hello.wasm
```

Run Spacegate with the demo config from the repository root:

```bash
RUST_LOG=info cargo run -p spacegate --features wasm -- -c file:resource/wasm-hello-demo
```

On startup, Spacegate should log:

```text
hello world from spacegate wasm plugin
hello world wasm plugin configured
```

The demo route also lets the plugin return a direct response:

```bash
curl http://127.0.0.1:18082/hello-world
```
