FROM node:24.22.1-bookworm-slim AS build
ARG SQLITE_PREBUILD_MIRROR=
ENV npm_config_better_sqlite3_binary_host_mirror=$SQLITE_PREBUILD_MIRROR

ENV PNPM_HOME=/pnpm
ENV PATH=$PNPM_HOME:$PATH
ENV CI=true
WORKDIR /workspace

RUN corepack enable && corepack prepare pnpm@12.11.2 --activate && pnpm config set store-dir /pnpm/store
COPY pnpm-lock.yaml pnpm-workspace.yaml ./
COPY images/dependencies/ ./
RUN --mount=type=cache,id=spacegate-admin-pnpm,target=/pnpm/store pnpm fetch --ignore-scripts
COPY . .
RUN --mount=type=cache,id=spacegate-admin-pnpm,target=/pnpm/store pnpm install --offline --frozen-lockfile --ignore-scripts
RUN pnpm --filter spacegate-admin-shared paraglide:compile && pnpm --filter spacegate-admin-web... --if-present build

FROM nginxinc/nginx-unprivileged:1.29.8-alpine AS web

ENV PUBLIC_API_BASE=
ENV NGINX_ENVSUBST_FILTER=PUBLIC_API_BASE_BASE64
ENV NGINX_ENVSUBST_OUTPUT_DIR=/tmp/hai-runtime-config

RUN mkdir -p /tmp/hai-runtime-config
COPY images/web/nginx.conf /etc/nginx/conf.d/default.conf
COPY --chmod=755 images/web/19-runtime-config.envsh /docker-entrypoint.d/19-runtime-config.envsh
COPY images/web/runtime-config.js.template /etc/nginx/templates/runtime-config.js.template
COPY --from=build /workspace/apps/spacegate-admin-web/dist /usr/share/nginx/html

EXPOSE 8080
HEALTHCHECK --interval=30s --timeout=5s --start-period=10s --retries=3 \
  CMD wget -Y off -q -O /dev/null http://127.0.0.1:8080/health || exit 1
