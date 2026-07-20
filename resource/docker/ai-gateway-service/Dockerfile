# ai-gateway-service K8s 镜像（多阶段：Rust 编译 + 最小运行层）
ARG RUST_IMAGE=rust:1.88-bookworm
ARG RUNTIME_IMAGE=debian:bookworm-slim

FROM ${RUST_IMAGE} AS builder
WORKDIR /src
COPY . .
# Install native build dependencies required by openssl-sys.
RUN apt-get update \
  && apt-get install -y --no-install-recommends pkg-config libssl-dev \
  && rm -rf /var/lib/apt/lists/*
RUN cargo build --release -p ai-gateway-service

FROM ${RUNTIME_IMAGE}
RUN apt-get update && apt-get install -y --no-install-recommends ca-certificates libssl3 tini \
  && rm -rf /var/lib/apt/lists/*
COPY --from=builder /src/target/release/ai-gateway-service /usr/local/bin/ai-gateway-service
EXPOSE 18080
ENTRYPOINT ["/usr/bin/tini", "--"]
CMD ["/usr/local/bin/ai-gateway-service"]
