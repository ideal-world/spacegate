# 将已构建的网关二进制独立交付；不重新编译，不包含管理后台。
ARG GATEWAY_ARTIFACT_IMAGE=spacegate:v2026.11.06-001
FROM ${GATEWAY_ARTIFACT_IMAGE} AS artifact
ARG GATEWAY_BINARY=/usr/local/bin/spacegate
RUN test -x "$GATEWAY_BINARY" && cp "$GATEWAY_BINARY" /tmp/spacegate-runtime

FROM debian:bookworm-slim
RUN apt-get update && apt-get install -y --no-install-recommends ca-certificates libssl3 tini \
    && rm -rf /var/lib/apt/lists/* \
    && mkdir -p /var/lib/spacegate/plugins /plugins
COPY --from=artifact /tmp/spacegate-runtime /usr/local/bin/spacegate
COPY --from=artifact /lib/spacegate/wasm /lib/spacegate/wasm
ENV RUST_LOG=info
ENTRYPOINT ["/usr/bin/tini", "-g", "--", "/usr/local/bin/spacegate"]
