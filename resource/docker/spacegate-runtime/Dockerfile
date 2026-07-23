FROM debian:bookworm-slim
LABEL org.opencontainers.image.source="https://github.com/ideal-world/spacegate"

RUN apt-get update \
    && apt-get install -y --no-install-recommends ca-certificates libssl3 tini \
    && rm -rf /var/lib/apt/lists/*

COPY spacegate /usr/local/bin/spacegate
COPY spacegate_plugin_ai_gateway_queue.wasm /lib/spacegate/wasm/spacegate_plugin_ai_gateway_queue.wasm

ENTRYPOINT ["/usr/bin/tini", "--", "/usr/local/bin/spacegate"]
