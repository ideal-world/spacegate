ARG RUST_IMAGE=rust:1.88-bookworm
ARG NGINX_IMAGE=nginx:1.27-bookworm

FROM ${RUST_IMAGE} AS builder
WORKDIR /app
COPY . .
# Install native build dependencies required by openssl-sys through reqwest.
RUN apt-get update \
    && apt-get install -y --no-install-recommends pkg-config libssl-dev \
    && rm -rf /var/lib/apt/lists/*
RUN cargo build --release -p spacegate-admin-server

FROM ${NGINX_IMAGE}
LABEL org.opencontainers.image.source="https://github.com/ideal-world/spacegate"

WORKDIR /usr/src/app

COPY resource/docker/spacegate-admin/dist /usr/share/nginx/html

COPY --from=builder /app/target/release/spacegate-admin-server /usr/src/app/admin-server
COPY resource/docker/spacegate-admin/nginx-default.conf /etc/nginx/nginx.conf
COPY resource/docker/spacegate-admin/start.sh /usr/src/app/start.sh
RUN chmod +x start.sh

# COPY libssl1.1_1.1.1f-1ubuntu2.20_amd64.deb /usr/src/app
# RUN  dpkg -i libssl1.1_1.1.1f-1ubuntu2.20_amd64.deb

EXPOSE 9080

CMD ["sh", "-c", "sh start.sh"]
