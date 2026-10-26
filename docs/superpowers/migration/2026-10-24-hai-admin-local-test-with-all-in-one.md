# 用 all-in-one 镜像本地联调新 Hai 管理面

本文基于根目录 `readme.md` 的 all-in-one 方案，给出一套**同时运行旧管理端与新 Hai 管理面**的本地测试流程。
all-in-one 继续提供 SpaceGate 网关、Rust `spacegate-admin-server`、AI Gateway 和旧 Vue 管理端；
新 Hai service/web 作为独立进程连接同一个上游，用于逐页对比和回归。

> 旧 all-in-one 镜像里的前端仍是 `spacegate-admin-fe`。本文不会修改它，也不会把新前端打进旧镜像。
> 新前端通过 `pnpm dev` 单独启动。

## 1. 本地拓扑

```text
旧 Vue 管理端  http://127.0.0.1:9080
        │  /api -> 容器内 9992
        ▼
┌──────────────────────── spacegate-all-in-one ────────────────────────┐
│ nginx 9080 │ Rust admin-server 9992 │ AI Gateway 18080 │ 网关 9991/9876 │
└──────────────────────────────────────────────────────────────────────┘
        ▲                                   ▲
        │ HAI_SPACEGATE_ADMINBASEURL        │ HAI_SPACEGATE_AIGATEWAYBASEURL
        │                                   │
新 Hai service  http://127.0.0.1:3000  ─────┘
        ▲
        │ /api/v1（SM2）
新 Web/App/Desktop  http://127.0.0.1:4173 / 5175 / 5176
```

- 旧 UI：`9080`，旧登录方式，保留作对照。
- 新 UI：`4173`，Hai IAM 登录（默认 `admin` + `.env.local` 中的初始密码）。
- 两端写的是同一份 Rust 配置，因此可以在新 UI 创建/修改，再回旧 UI 验证。
- 新 service 默认 `3000`；小程序通道可选 `3001`。

## 2. 一次性准备

### 2.1 构建 all-in-one 镜像

```bash
cd /Users/yiye/projectSpace/huayun_project/spacegate-workspace

export HAI_HUB_ROOT="/Users/yiye/projectSpace/huayun_project/hai-workspace/hai-hub"

docker build --build-context "hai_hub=$HAI_HUB_ROOT" \
  -f Dockerfile.all-in-one -t spacegate-all-in-one:local .
```

镜像源不可用时，按根 `readme.md` 覆盖 `NODE_IMAGE` / `RUST_IMAGE` / `RUNTIME_IMAGE`。

如果出现 `no space left on device` 且日志包含 `transferring context: xxGB`：

```bash
docker builder prune
```

根 `.dockerignore` 已排除 `target/`、`node_modules/` 等目录；仍失败时先确认构建上下文是工作区根目录。

### 2.2 启动外部 Redis

all-in-one 需要外部 Redis，未就绪会直接退出，不会降级到临时数据。

```bash
docker run -d --rm --name spacegate-redis \
  -p 6379:6379 \
  redis:7-alpine
```

已有本机 Redis 可跳过，后面把 `REDIS_URL` 指向它。

### 2.3 准备干净的测试卷

建议使用独立的测试 volume，避免污染现有 `spacegate-config`：

```bash
docker volume create spacegate-config-test
docker volume create spacegate-native-plugins-test
docker volume create spacegate-wasm-test
```

## 3. 启动 all-in-one（旧栈 + 上游）

在根 `readme.md` 的命令上增加两个端口：`9992`（新 service 要直连 Rust admin-server）和
`18080`（新 service 要直连 AI Gateway 管理接口）。

```bash
docker run --rm \
  --name spacegate-all-in-one \
  -p 9080:9080 \
  -p 9991:9991 \
  -p 9876:9876 \
  -p 9000:9000 \
  -p 9992:9992 \
  -p 18080:18080 \
  --add-host=host.docker.internal:host-gateway \
  -e REDIS_URL=redis://host.docker.internal:6379/0 \
  -v spacegate-config-test:/etc/spacegate \
  -v spacegate-native-plugins-test:/var/lib/spacegate/plugins \
  -v spacegate-wasm-test:/plugins \
  spacegate-all-in-one:local
```

验证：

```bash
curl -fsS http://127.0.0.1:9992/config/names
curl -fsS http://127.0.0.1:18080/v1/admin/plugins/ai-gateway-queue/schema
open http://127.0.0.1:9080
```

说明：

- all-in-one 启动 `spacegate-admin-server` 时**没有** `--key` / `--sk`，所以上游认证关闭。
  新 service 必须显式设置 `HAI_SPACEGATE_ALLOWINSECURE=true`，不能依赖自动降级。
- `9992` 只用于本机联调；生产必须走内网/HTTPS 并启用 ak/sk。
- 旧 UI、新 service 共用 `/etc/spacegate` 配置卷，测试数据可随时丢弃。

## 4. 配置并启动新管理面

### 4.1 写入本地私有配置

编辑 `spacegate/spacegate-admin/.env.local`（已被 Git 忽略），追加/确认：

```dotenv
# 连接本机 all-in-one 的 Rust admin-server；无认证上游显式允许
HAI_SPACEGATE_ADMINBASEURL=http://127.0.0.1:9992
HAI_SPACEGATE_AIGATEWAYBASEURL=http://127.0.0.1:18080
HAI_SPACEGATE_ALLOWINSECURE=true

# 不设置 ADMINAK/ADMINSK；如果以后给 all-in-one 开启 --key/--sk，再改成：
# HAI_SPACEGATE_ADMINAK=...
# HAI_SPACEGATE_ADMINSK=...
# HAI_SPACEGATE_ALLOWINSECURE=false

# 可选：微信小程序通道
# HAI_SERV_MINIAPP_PORT=3001
```

同时确认 `.env.local` 已有：

- `HAI_ADMIN_DEFAULT_PASSWORD`：新管理面首次登录密码。
- `HAI_CRYPTO_TRANSPORT_SERVERKEYRING`：主通道 SM2 密钥环；缺失时可用
  `node deploy/generate-transport-keyring.mjs` 生成后写入。

### 4.2 启动

```bash
cd /Users/yiye/projectSpace/huayun_project/spacegate-workspace/spacegate/spacegate-admin
pnpm install
pnpm dev
```

入口：

| 入口 | 地址 | 登录 |
| --- | --- | --- |
| 新 Web | <http://127.0.0.1:4173> | `admin` / `.env.local` 的 `HAI_ADMIN_DEFAULT_PASSWORD` |
| 新 App 预览 | <http://127.0.0.1:5175> | 同上 |
| 新 Desktop 预览 | <http://127.0.0.1:5176> | 同上 |
| 旧 Web | <http://127.0.0.1:9080> | 旧 ak/sk 流程（无认证上游时可直接进入） |
| 新 service | <http://127.0.0.1:3000/health> | 健康检查公开 |
| 小程序通道 | <http://127.0.0.1:3001/api/miniapp/v1> | 仅设置 `HAI_SERV_MINIAPP_PORT` 后 |

> 统一使用 `127.0.0.1`，不要在新旧入口之间混用 `localhost`，避免 Cookie/Origin 差异造成误判。

## 5. 联调清单

### 5.1 只读对齐

在新 UI 依次打开：

1. **概览**：网关数、路由数、插件实例数、实例在线数。
2. **网关**：监听器、TLS、Redis 连接池、网关插件绑定。
3. **路由**：规则匹配、重写、后端、MCP 传输、三层插件绑定。
4. **插件**：HAI 原生插件目录、实例、展示名称、Wasm schema。
5. **AI 队列**：规则列表、schema、readme。
6. **实例**：实例列表与健康状态。

与旧 UI `9080` 对照，数量、名称、字段应一致。差异记录到
`docs/superpowers/progress/2026-10-24-legacy-admin-operation-inventory.md` 的回归表。

### 5.2 写操作对齐

建议用独立测试网关，避免破坏默认配置：

1. 新 UI 创建 `local-test-gateway`，配置一个 listener、Redis URL。
2. 旧 UI 刷新，确认能看到该网关。
3. 新 UI 创建 `local-test-route`，绑定后端与插件。
4. 旧 UI 验证路由规则、后端、插件顺序。
5. 新 UI 重命名路由，确认旧 UI 看到新名称且规则/后端未丢。
6. 删除测试路由和网关，旧 UI 确认消失。

### 5.3 版本冲突 409

1. 新 UI 打开一个网关/路由编辑器，先不要保存。
2. 在另一个新 UI 标签页（或旧 UI）修改同一资源并保存。
3. 回到第一个标签页保存，应出现 **版本冲突** 横幅，且不会自动重试。
4. 点击“刷新后重试”，重新读取最新配置，再保存成功。
5. 确认没有静默覆盖对方修改。

### 5.4 插件与 Wasm

- 在插件页执行“刷新”，确认 HAI 内置插件（`hai-auth`、`hai-asset`、`hai-quota`、`hai-dispatch`）出现。
- 编辑插件展示名称，保存后旧 UI/新 UI 都能看到。
- 预览 Wasm schema：分别测试 `file:///plugins/...`、HTTP(S)、OCI 来源；HTTP(S) 认证头只在服务端使用。
- 不要把内置插件目录 `/lib/spacegate/plugins`、`/lib/spacegate/wasm` 挂成 volume。

### 5.5 AI 队列

前提：Redis 可用，AI Gateway 已启动。

- 读取 `ai-gateway-queue` schema/readme。
- 新建/编辑/删除租户限流规则，检查列表。
- 使用 resolve 查询命中规则，确认 policy/rps/burst/cost/fallback。
- 旧 UI 的 AI 队列页应看到同一份规则。

### 5.6 实例与重载

- 查看实例列表与健康状态。
- 按网关/全局执行 reload；再刷新配置确认版本变化。
- 注意：Rust 把 reload POST 也纳入版本控制，reload 后其他标签页的下一次写可能 409，这是预期行为。

### 5.7 IAM 与权限

- 新 UI 退出/重新登录；密码登录应正常。
- 管理员创建普通 `user` 账号；该账号登录后只能看到个人设置，直接访问 `#/gateways` 应回退。
- 普通用户调用管理 procedure 应返回 403。
- LDAP 需要独立 LDAP 服务；本地 all-in-one 不提供。启用方式见
  `spacegate-admin/packages/spacegate-admin-serv/config/_iam.yml`。

### 5.8 小程序

1. 在 `.env.local` 设置 `HAI_SERV_MINIAPP_PORT=3001`，重启 service。
2. 微信开发者工具导入 `spacegate-admin/apps/spacegate-admin-miniapp`。
3. `project.private.config.json` 填真实 AppID；本地关闭域名校验（`urlCheck: false`）。
4. 默认 API：`http://127.0.0.1:3001/api/miniapp/v1`；真机需改为局域网/HTTPS 地址。
5. 覆盖：登录、概览、网关写、路由写、409 重新加载、退出登录。

小程序通道没有 SM2，必须 HTTPS/内网反代；本地仅用于开发验证。

## 6. 重置测试数据

只重置 Rust 配置/插件（保留新管理面 IAM）：

```bash
docker rm -f spacegate-all-in-one
docker volume rm spacegate-config-test spacegate-native-plugins-test spacegate-wasm-test
# 重新 create volume 后再按第 3 节启动
```

同时重置新管理面 IAM/数据库（会重新 bootstrap 管理员）：

```bash
cd spacegate/spacegate-admin
rm -rf packages/spacegate-admin-serv/data
# 重新 pnpm dev
```

Redis 测试数据：

```bash
docker exec spacegate-redis redis-cli FLUSHDB
```

## 7. 常见问题

| 现象 | 原因/处理 |
| --- | --- |
| `no space left on device` / `transferring context: xxGB` | 构建上下文含 `target/`/`node_modules/`；确认 `.dockerignore`，`docker builder prune` |
| all-in-one 启动即退出 | Redis 未就绪；先启动 Redis，检查 `REDIS_URL` 与 `--add-host` |
| 新 service 连接上游失败 | 没有发布 `-p 9992:9992` / `-p 18080:18080`，或地址写成了容器内 `127.0.0.1` |
| 新 service 报 `adminAk/adminSk are required` | 上游无认证时必须显式 `HAI_SPACEGATE_ALLOWINSECURE=true`；不要静默降级 |
| 旧 UI 能看到、新 UI 版本冲突 | 两端共享配置；先在新 UI 点“刷新后重试”，不要自动重试 |
| 登录后立刻 401/回登录页 | 混用了 `localhost`/`127.0.0.1`，或 service 重启导致会话失效 |
| 小程序请求失败 | 未设置 `HAI_SERV_MINIAPP_PORT`、未重启 service、或 DevTools 未关闭域名校验 |
| 旧 volume 挂到 `/lib/spacegate/plugins` 后 HAI 插件消失 | 内置目录不能挂 volume；按根 `readme.md` 迁移到 `/var/lib/spacegate/plugins` |

## 8. 与旧前端的关系

- `spacegate-admin-fe` 仍被打进当前 all-in-one 镜像，作为旧 UI 和回滚入口；本文不修改它。
- `spacegate-admin-front` 只是外部参考仓库，不参与构建。
- 新管理面验收通过并完成显式切换后，旧 `fe` 才可退役；退役前保留一个回滚窗口。
- 具体退役条件见 `2026-10-24-spacegate-admin-independent-deploy-and-switch.md`。
