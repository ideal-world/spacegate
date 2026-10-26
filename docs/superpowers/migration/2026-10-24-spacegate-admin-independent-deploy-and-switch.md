# SpaceGate Hai 管理面迁移：独立部署与切换说明

- 分支：`feat/hai-admin-migration`（仅本地提交，未 push）
- 新工程：`spacegate/spacegate-admin/`
- 旧管理端：`spacegate-admin-fe/`（只读基线，继续可用）
- 外部参考：`spacegate-admin-front/`（不作为验收对象，未修改）
- 设计文档：`docs/superpowers/specs/2026-10-15-spacegate-hai-framework-migration-design.md`

## 1. 边界

本次交付**新控制面**，不重写 Rust 数据平面，不删除旧 Vue，不执行生产切换。

```text
浏览器 / Capacitor / Tauri ─┐
                            ├─> spacegate-admin-serv (/api/v1, SM2)
微信小程序 ─────────────────┘        ├─> 既有 Rust admin-server
                                     └─> 既有 AI Gateway (/v1/admin)
```

- 新 service 负责 Hai IAM、contract、typed procedures、上游适配和多端聚合。
- Rust admin-server 继续负责配置读写、插件发现、实例健康、版本控制和运行时重载。
- 旧 Vue 登录（ak/sk + 可选 JWT）和旧部署入口保留到显式切换完成。
- 新工程使用独立数据库/会话；不迁移旧 `ak/sk`，不修改旧用户库。

## 2. 启动入口

### 2.1 本地源码

```sh
cd spacegate/spacegate-admin
pnpm install
pnpm dev
```

必需服务端变量：

| 变量 | 说明 |
| --- | --- |
| `HAI_ADMIN_DEFAULT_PASSWORD` | 首次启动管理员密码；已有用户时不重设 |
| `HAI_CRYPTO_TRANSPORT_SERVERKEYRING` | SM2 传输密钥环；本地可用 `node deploy/generate-transport-keyring.mjs` |
| `HAI_SPACEGATE_ADMINBASEURL` | Rust admin-server 基地址，仅内网 |
| `HAI_SPACEGATE_ADMINAK` / `HAI_SPACEGATE_ADMINSK` | 服务身份，成对配置 |
| `HAI_SPACEGATE_AIGATEWAYBASEURL` | AI Gateway 管理接口基地址 |
| `HAI_SPACEGATE_ALLOWINSECURE` | 仅显式无认证的本地上游设为 `true` |
| `HAI_SERV_MINIAPP_PORT` | 可选；设置后监听 `/api/miniapp/v1`，不设置即关闭 |

LDAP 变量见 `spacegate-admin/packages/spacegate-admin-serv/config/_iam.yml` 与 service README。
公开注册默认关闭；LDAP 首次登录同步为普通 `user`，管理员必须显式授予 `admin`。

### 2.2 独立容器

参见 `spacegate-admin/deploy/README.md`：

```sh
cd spacegate/spacegate-admin
cp deploy/spacegate-admin.env.example deploy/spacegate-admin.env
node deploy/generate-transport-keyring.mjs
docker compose -f deploy/compose.yaml up -d --build
```

评估栈使用 SQLite + 内存缓存，适合单实例；生产多实例必须使用共享 PostgreSQL/Redis/S3/vecdb，
并沿用既有 `hai deploy` 或发布流水线。

### 2.3 反向代理

推荐同源：

```text
/                         -> web 静态站点
/api/v1/*                 -> service:3000/api/v1/*
/api/miniapp/v1/*         -> service:3001/api/miniapp/v1/*   # 可选
```

主通道必须保留 SM2 传输；小程序通道没有 SM2，必须 HTTPS 终止、仅内网/受信反代可达。
不要缓存 `/api/*`，不要剥离 `Authorization`、`X-Client-Id`、`X-Encrypted`、`X-Hai-Transport`、
`X-Request-Client`、`X-HAI-Token-Transport`。

## 3. 切换步骤

1. **验收**：完成第 5 节命令与浏览器验收；原生 App/Desktop/小程序验收单独报告。
2. **预热**：构建新 service/web 镜像，部署到独立域名或路径；不覆盖旧镜像 tag、ingress、DNS。
3. **连通性**：用同一 Rust admin-server/AI Gateway 做只读验证：概览、网关/路由列表、实例健康。
4. **写操作**：在非生产上游或测试配置上验证网关/路由写、409 冲突、显式刷新后重试。
5. **切换**：由发布人员显式把入口路由切到新 Web/service；旧入口保持在线。
6. **观察**：检查 service 日志（不得出现 token/密码）、IAM audit、上游 401/409 指标。
7. **保留回滚窗口**：旧 Vue、旧镜像和旧路由至少保留一个发布周期。

## 4. 回滚

- 把 ingress/DNS/镜像 tag 指回旧 Vue 与旧部署。
- 新控制面可整体下线；它使用独立数据库/会话，不会改写旧配置。
- 不删除旧前端、旧登录路径或旧部署清单。
- 若已产生新配置写入，先确认 Rust admin-server 的版本与配置快照，再决定是否回滚数据。

## 5. 验证矩阵

| 范围 | 命令/证据 | 当前结果 |
| --- | --- | --- |
| Contract | `pnpm --filter spacegate-admin-contract typecheck/test/lint` | 3 tests 通过 |
| Service 上游 | `pnpm --filter spacegate-admin-serv test` | 41 tests 通过（含 13 上游、7 LDAP、3 小程序通道） |
| Shared | `pnpm --filter spacegate-admin-shared typecheck/test/lint` | 20 tests 通过 |
| Miniapp mock | `pnpm --filter spacegate-admin-miniapp typecheck/test` | 10 tests 通过；原生 DevTools 未执行 |
| 全仓静态 | `pnpm typecheck` / `pnpm lint` | 通过 |
| 浏览器 | `pnpm test:e2e` | 16 passed, 1 skipped（OTP 需通知夹具） |
| 构建 | `pnpm build` / `pnpm build --target docker` | 待最终复跑 |
| Rust 上游 | `cargo test -p spacegate-admin-server` | 待最终复跑 |
| 原生打包 | Capacitor/Tauri/微信开发者工具 | 未执行，不能声称原生通过 |

## 6. 已知限制与风险

- 小程序通道关闭了 SM2 传输加密，以 HTTPS + IAM + `admin` 守卫补偿；生产必须限制网络与反代。
- 小程序/管理端复杂表单允许 JSON 文本域编辑嵌套结构，但顶层专用字段和校验仍保留；不是通用 JSON 替代品。
- Rust 写成功不返回新版本，service 通过写后 GET 读取版本；409 不自动重试。
- 编辑版本与资源快照绑定，后台读取推进全局版本时不会用旧草稿提交新版本。
- 旧 Vue 的未提交改动和 `components/config` 子模块保持原样；新工程不依赖其运行。
- 本次未执行生产切换、未迁移旧用户库、未 push 远端。

## 7. 后续接续

进度看板：`docs/superpowers/progress/2026-10-24-hai-admin-migration-progress.md`
旧 UI 回归基线：`docs/superpowers/progress/2026-10-24-legacy-admin-operation-inventory.md`

接续顺序建议：最终全量构建/Rust 回归 → 原生 App/Desktop 打包 → 微信开发者工具真机验收 →
生产镜像签名与部署演练 → 显式切换。
