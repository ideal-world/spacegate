FROM node:24.22.1-bookworm-slim AS build
ARG SQLITE_PREBUILD_MIRROR=
ENV npm_config_better_sqlite3_binary_host_mirror=$SQLITE_PREBUILD_MIRROR

ENV PNPM_HOME=/pnpm
ENV PATH=$PNPM_HOME:$PATH
ENV CI=true
ENV ONNXRUNTIME_NODE_INSTALL_CUDA=skip
WORKDIR /workspace

RUN corepack enable && corepack prepare pnpm@12.11.2 --activate && pnpm config set store-dir /pnpm/store
COPY pnpm-lock.yaml pnpm-workspace.yaml ./
COPY images/dependencies/ ./
RUN --mount=type=cache,id=spacegate-admin-pnpm,target=/pnpm/store pnpm fetch --ignore-scripts
COPY . .
RUN --mount=type=cache,id=spacegate-admin-pnpm,target=/pnpm/store pnpm install --offline --frozen-lockfile --ignore-scripts
RUN pnpm --filter spacegate-admin-serv... --if-present build

RUN --mount=type=cache,id=spacegate-admin-sqlite,target=/root/.npm/_prebuilds if [ -d node_modules/better-sqlite3 ]; then cd node_modules/better-sqlite3; for attempt in 1 2 3; do node -e "require('node:child_process').execFileSync(process.execPath,[require.resolve('prebuild-install/bin.js'),'--verbose'],{stdio:'inherit'})" && break; [ "$attempt" = 3 ] && exit 1; sleep 3; done; fi
RUN --mount=type=cache,id=spacegate-admin-pnpm,target=/pnpm/store --mount=type=cache,id=spacegate-admin-sqlite,target=/root/.npm/_prebuilds pnpm pm deploy --filter spacegate-admin-serv --prod /prod/service

FROM node:24.22.1-bookworm-slim AS service

ENV NODE_ENV=production
ENV HOST=0.0.0.0
ENV PORT=3000
WORKDIR /app

COPY --from=build --chown=node:node /prod/service ./
RUN mkdir -p /app/data && chown node:node /app/data

USER node
EXPOSE 3000
HEALTHCHECK --interval=30s --timeout=5s --start-period=15s --retries=3 \
  CMD node -e "fetch('http://127.0.0.1:'+process.env.PORT+'/health').then(r=>process.exit(r.ok?0:1)).catch(()=>process.exit(1))"
CMD ["node", "dist/index.js"]
