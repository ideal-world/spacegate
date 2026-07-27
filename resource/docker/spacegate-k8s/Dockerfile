FROM rust:1.88-bookworm AS builder

WORKDIR /app
RUN apt-get update \
    && apt-get install -y --no-install-recommends ca-certificates pkg-config libssl-dev perl make cmake clang \
    && rm -rf /var/lib/apt/lists/*

# Avoid failing Rust dependency downloads on slow but still progressing registry connections.
ENV CARGO_NET_RETRY=10 \
    CARGO_HTTP_TIMEOUT=600 \
    CARGO_HTTP_LOW_SPEED_LIMIT=1 \
    CARGO_PROFILE_RELEASE_LTO=false \
    CARGO_PROFILE_RELEASE_PANIC=unwind

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
# Resolve shared Rust crates from the same lockfile as the embedded gateway runtime.
COPY --from=builder /app/Cargo.lock /hai-hub/Cargo.lock
RUN mkdir -p /hai-hub/.cargo
# Keep the static HAI gateway member and trim only unrelated workspace members
# whose path dependencies are not part of the gateway image build.
RUN sed -i '/"backend\/hai-hub-resource"/d;/"services\/hai-hub-all"/d;/"services\/hai-hub-auth-plugin"/d' /hai-hub/Cargo.toml
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
WORKDIR /hai-hub
RUN echo "==> hai-hub workspace members after trim" \
    && sed -n '1,12p' /hai-hub/Cargo.toml \
    && echo "==> local SpaceGate patch config" \
    && cat /hai-hub/.cargo/config.toml
RUN --mount=type=cache,target=/usr/local/cargo/registry \
    --mount=type=cache,target=/usr/local/cargo/git \
    --mount=type=cache,target=/hai-hub/target \
    cargo build --manifest-path /hai-hub/Cargo.toml --release -p hai-hub-spacegate --config /hai-hub/.cargo/config.toml \
    && install -Dm755 target/release/hai-hub-spacegate /hai-plugin/hai-hub-spacegate

FROM debian:bookworm-slim

RUN apt-get update \
    && apt-get install -y --no-install-recommends ca-certificates tini \
    && mkdir -p /lib/spacegate/plugins \
    && rm -rf /var/lib/apt/lists/*

COPY --from=hai-plugin-builder /hai-plugin/hai-hub-spacegate /usr/local/bin/spacegate
COPY --from=builder /app/plugins/wasm/target/wasm32-wasip1/release/spacegate_plugin_ai_gateway_queue.wasm /lib/spacegate/wasm/spacegate_plugin_ai_gateway_queue.wasm

ENV STARTUP_MODE=k8s
ENV K8S_NAMESPACE=ai-hai
ENV PLUGIN_DIR=/lib/spacegate/plugins
ENV RUST_LOG=info

EXPOSE 80 443 9993
ENTRYPOINT ["/usr/bin/tini", "--", "/usr/local/bin/spacegate"]
