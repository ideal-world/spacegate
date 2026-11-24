# 单独编译静态 HAI 网关；不构建管理后台、旧 Vue 或 Wasm 服务。
ARG RUST_IMAGE=mirror.gcr.io/library/rust:1.88-bookworm
FROM ${RUST_IMAGE} AS builder
ARG CARGO_BUILD_JOBS=2
WORKDIR /app
RUN apt-get update \
    && apt-get install -y --no-install-recommends ca-certificates pkg-config libssl-dev perl make cmake clang \
    && rm -rf /var/lib/apt/lists/*
ENV CARGO_NET_RETRY=5 CARGO_HTTP_TIMEOUT=300 CARGO_HTTP_LOW_SPEED_LIMIT=1 \
    CARGO_PROFILE_RELEASE_LTO=false CARGO_PROFILE_RELEASE_PANIC=unwind
COPY Cargo.toml Cargo.lock ./
COPY crates ./crates
COPY plugins ./plugins
COPY binary ./binary
COPY examples ./examples
# 只复制网关所需的 Rust 工程，避免把 Hub 私有环境和运行数据放入构建层。
COPY --from=hai_hub Cargo.toml Cargo.lock /hai-hub/
COPY --from=hai_hub backend/hai-hub-spacegate-plugins /hai-hub/backend/hai-hub-spacegate-plugins
COPY --from=hai_hub services/hai-hub-spacegate /hai-hub/services/hai-hub-spacegate
RUN sed -i '/"backend\/hai-hub-observe"/d;/"backend\/hai-hub-resource"/d;/"services\/hai-hub-all"/d;/"services\/hai-hub-auth-plugin"/d;/"services\/hai-hub-diagnostics"/d' /hai-hub/Cargo.toml \
    && mkdir -p /hai-hub/.cargo \
    && printf '%s\n' \
    '[patch."https://github.com/ideal-world/spacegate"]' \
    'spacegate-config = { path = "/app/crates/config" }' \
    'spacegate-ext-axum = { path = "/app/crates/extension/axum" }' \
    'spacegate-ext-redis = { path = "/app/crates/extension/redis" }' \
    'spacegate-kernel = { path = "/app/crates/kernel" }' \
    'spacegate-model = { path = "/app/crates/model" }' \
    'spacegate-plugin = { path = "/app/crates/plugin" }' \
    'spacegate-shell = { path = "/app/crates/shell" }' > /hai-hub/.cargo/config.toml
WORKDIR /hai-hub
# 仅影响构建；断网时可显式复用已完整下载的 Cargo 缓存。
ARG CARGO_NET_OFFLINE=false
RUN --mount=type=cache,target=/usr/local/cargo/registry \
    --mount=type=cache,target=/usr/local/cargo/git \
    --mount=type=cache,target=/hai-hub/target \
    cargo build --release --jobs "$CARGO_BUILD_JOBS" -p hai-hub-spacegate --config /hai-hub/.cargo/config.toml \
    && install -Dm755 target/release/hai-hub-spacegate /out/spacegate

FROM debian:bookworm-slim
RUN apt-get update && apt-get install -y --no-install-recommends ca-certificates libssl3 tini \
    && rm -rf /var/lib/apt/lists/* \
    && mkdir -p /var/lib/spacegate/plugins /plugins
COPY --from=builder /out/spacegate /usr/local/bin/spacegate
ENV RUST_LOG=info
ENTRYPOINT ["/usr/bin/tini", "--", "/usr/local/bin/spacegate"]
