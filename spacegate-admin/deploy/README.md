# SpaceGate Hai 管理面独立启动与切换

本目录只提供**新控制面**的独立启动、镜像和切换说明，不修改旧 Vue 管理端
（`../../spacegate-admin-fe`）或现有 SpaceGate 部署。生产切换由部署方显式选择新镜像/路由，
本分支不执行任何生产切换。

本地同时运行旧 all-in-one 与新管理面的完整步骤，见
[`docs/superpowers/migration/2026-10-24-hai-admin-local-test-with-all-in-one.md`](../../docs/superpowers/migration/2026-10-24-hai-admin-local-test-with-all-in-one.md)。

## 架构与端口

```text
Web / Capacitor / Tauri ──HTTPS──> /api/v1 (SM2 transport) ──> Rust admin-server (REST)
                                              └──────────────> AI Gateway (/v1/admin)
微信小程序 ───────────────HTTPS──> /api/miniapp/v1 (no SM2, IAM + admin guard)
```

| 组件 | 默认端口 | 说明 |
| --- | --- | --- |
| `spacegate-admin-serv` 主通道 | `3000` | `/api/v1`，业务 JSON 强制 SM2 transport；浏览器 refresh Cookie |
| 小程序通道 | `3001` | `/api/miniapp/v1`，仅 `HAI_SERV_MINIAPP_PORT` 设置时启用；必须 HTTPS 反代 |
| Web 静态站点 | `8080`（容器）/`4173`（开发） | 运行时 `PUBLIC_API_BASE` 指向 service |
| Rust admin-server | 由部署方决定 | 既有上游，不在此栈内启动 |
| AI Gateway | 由部署方决定 | 既有上游，不在此栈内启动 |

## 1. 本地源码启动

前置：Node `^22.22.2 || ^24.15.0 || >=26.0.0`、pnpm `>=12.11.2`。

```sh
cd spacegate/spacegate-admin
pnpm install

# 私有值放 .env.local；不要提交
# 首次启动需要：
#   HAI_ADMIN_DEFAULT_PASSWORD
#   HAI_CRYPTO_TRANSPORT_SERVERKEYRING（可运行 node deploy/generate-transport-keyring.mjs）
#   HAI_SPACEGATE_ADMINBASEURL / HAI_SPACEGATE_ADMINAK / HAI_SPACEGATE_ADMINSK
#   HAI_SPACEGATE_AIGATEWAYBASEURL
pnpm dev
```

- Web：`http://127.0.0.1:4173`
- App/Desktop 浏览器预览：`5175` / `5176`
- 小程序通道：在 `.env.local` 设置 `HAI_SERV_MINIAPP_PORT=3001` 后重启 service；不设置即完全关闭。
- 本地未启用上游认证时，必须显式设置 `HAI_SPACEGATE_ALLOWINSECURE=true`，不能在认证失败后自动降级。

## 2. 容器独立启动（评估栈）

```sh
cd spacegate/spacegate-admin
cp deploy/spacegate-admin.env.example deploy/spacegate-admin.env
# 编辑 deploy/spacegate-admin.env：生成 keyring、填写上游地址/凭据、管理员初始密码
node deploy/generate-transport-keyring.mjs

docker compose -f deploy/compose.yaml build
docker compose -f deploy/compose.yaml up -d
```

- Web：`http://127.0.0.1:8080`
- Service：`http://127.0.0.1:3000`
- 小程序通道：`http://127.0.0.1:3001/api/miniapp/v1`（仅环境文件设置端口时）
- 上游 Rust/AI Gateway 地址使用 `host.docker.internal`；Linux 已配置 `host-gateway` 映射。
- 该 compose 使用 SQLite + 内存缓存，适合单实例评估；**不是生产多实例方案**。

## 3. 生产部署边界

生产使用 `pnpm exec hai deploy packages/spacegate-admin-serv --env prod ...` 或既有发布流水线，
并显式提供：

- 共享 PostgreSQL/MySQL、Redis、S3、vecdb；生产启动会拒绝本地 SQLite/内存/本地存储。
- 所有节点相同的 `HAI_CRYPTO_TRANSPORT_SERVERKEYRING` 与共享 key store。
- 仅内网可达的 `HAI_SPACEGATE_ADMINBASEURL`、`HAI_SPACEGATE_AI...`，以及服务身份 ak/sk。
- 显式 CORS Origin；Web 与 API 默认同源反代以保留 HttpOnly refresh Cookie。
- LDAP 凭据只通过 Secret 注入：`HAI_IAM_LDAP_BINDPASSWORD`。`register.enabled=false` 保持公开注册关闭，
  `register.defaultEnabled=true` 仅用于让 LDAP 同步用户可登录。
- 小程序通道如启用，必须 HTTPS 终止、只允许内网/受信反代访问，并通过 `HAI_SERV_MINIAPP_PORT` 显式开启。

## 4. 反向代理示例

同源部署（推荐）：

```text
https://admin.example.com/            -> web:8080 静态站点
https://admin.example.com/api/v1/*    -> service:3000/api/v1/*
https://admin.example.com/api/miniapp/v1/* -> service:3001/api/miniapp/v1/*   # 可选
```

必须保留 `Authorization`、`X-Request-Client`、`X-HAI-Token-Transport`、`X-Client-Id`、
`X-Encrypted`、`X-Hai-Transport` 等头；不要缓存 `/api/*`。小程序通道没有 SM2，只能依赖 HTTPS。

## 5. 切换与回滚

切换前：

1. 完成本地验收（见 `docs/superpowers/migration/2026-10-24-spacegate-admin-independent-deploy-and-switch.md`）。
2. 准备新 service/web 镜像与独立域名/路径，不覆盖旧 Vue 的镜像 tag、ingress 或 DNS。
3. 用同一上游 Rust admin-server/AI Gateway 做一次只读连通性验证。
4. 由发布人员显式把入口路由切到新 Web/service；旧入口保持可回滚。

回滚：

- 把 ingress/DNS/镜像 tag 指回旧 Vue 与旧部署即可；旧前端、旧登录路径和旧配置未删除。
- 新控制面使用独立 IAM 数据库/会话；不要迁移或改写旧 `ak/sk` 凭据。
- 本分支不执行生产切换，也不删除旧工程。

## 6. 验收命令

```sh
cd spacegate/spacegate-admin
pnpm typecheck
pnpm lint
pnpm test
pnpm build
pnpm test:e2e
```

Rust 上游回归：

```sh
cd spacegate
cargo test -p spacegate-admin-server
```

原生小程序/App/Desktop 打包和微信开发者工具验收必须单独执行并报告；浏览器预览不等于原生通过。
