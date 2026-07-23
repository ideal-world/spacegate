FROM rust:1.88-bookworm AS builder

WORKDIR /app
RUN apt-get update \
    && apt-get install -y --no-install-recommends ca-certificates pkg-config libssl-dev perl make cmake clang \
    && rm -rf /var/lib/apt/lists/*

# Avoid failing Rust dependency downloads on slow but still progressing registry connections.
ENV CARGO_NET_RETRY=10 \
    CARGO_HTTP_TIMEOUT=600 \
    CARGO_HTTP_LOW_SPEED_LIMIT=1

COPY . .
RUN cargo build --release -p spacegate --features build-k8s,wasm,dylib,static-openssl
RUN rustup target add wasm32-wasip1 \
    && cargo build --release \
      --target wasm32-wasip1 \
      --manifest-path plugins/wasm/Cargo.toml \
      -p spacegate_plugin_ai_gateway_queue \
    && test -f /app/plugins/wasm/target/wasm32-wasip1/release/spacegate_plugin_ai_gateway_queue.wasm

FROM builder AS hai-plugin-builder

COPY --from=hai_hub . /hai-hub
RUN mkdir -p /hai-hub/.cargo
RUN sed -i '/"backend\/hai-hub-resource"/d;/"services\/hai-hub-all"/d;/"services\/hai-hub-spacegate"/d;/"services\/hai-hub-auth-plugin"/d' /hai-hub/Cargo.toml
RUN sed -i 's/crate-type = \[ "rlib" \]/crate-type = [ "rlib", "dylib" ]/' /hai-hub/backend/hai-hub-spacegate-plugins/Cargo.toml \
    && grep -n 'crate-type = .*dylib' /hai-hub/backend/hai-hub-spacegate-plugins/Cargo.toml
RUN printf '%s\n' \
    '[patch."https://github.com/ideal-world/spacegate"]' \
    'spacegate-config = { path = "/app/crates/config" }' \
    'spacegate-ext-axum = { path = "/app/crates/extension/axum" }' \
    'spacegate-ext-redis = { path = "/app/crates/extension/redis" }' \
    'spacegate-kernel = { path = "/app/crates/kernel" }' \
    'spacegate-model = { path = "/app/crates/model" }' \
    'spacegate-plugin = { path = "/app/crates/plugin" }' \
    'spacegate-shell = { path = "/app/crates/shell" }' \
    > /hai-hub/.cargo/config.toml
RUN echo "==> hai-hub workspace members after trim" \
    && sed -n '1,12p' /hai-hub/Cargo.toml \
    && echo "==> hai-hub-spacegate-plugins lib config" \
    && sed -n '18,22p' /hai-hub/backend/hai-hub-spacegate-plugins/Cargo.toml \
    && echo "==> local SpaceGate patch config" \
    && cat /hai-hub/.cargo/config.toml
RUN CARGO_PROFILE_RELEASE_LTO=false CARGO_PROFILE_RELEASE_PANIC=unwind \
    cargo build --manifest-path /hai-hub/Cargo.toml --release -p hai-hub-spacegate-plugins --features schema \
    && test -f /hai-hub/target/release/libhai_hub_spacegate_plugins.so

FROM debian:bookworm-slim

RUN apt-get update \
    && apt-get install -y --no-install-recommends ca-certificates tini \
    && rm -rf /var/lib/apt/lists/*

COPY --from=builder /app/target/release/spacegate /usr/local/bin/spacegate
COPY --from=hai-plugin-builder /hai-hub/target/release/libhai_hub_spacegate_plugins.so /lib/spacegate/plugins/hai_hub_spacegate_plugins.so
COPY --from=builder /app/plugins/wasm/target/wasm32-wasip1/release/spacegate_plugin_ai_gateway_queue.wasm /lib/spacegate/wasm/spacegate_plugin_ai_gateway_queue.wasm

ENV CONFIG=k8s:spacegate
ENV RUST_LOG=info

EXPOSE 80 443 9993
ENTRYPOINT ["/usr/bin/tini", "--", "/usr/local/bin/spacegate"]
