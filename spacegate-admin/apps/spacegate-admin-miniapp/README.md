# spacegate-admin-miniapp

SpaceGate Hai 管理端的**微信原生小程序**实现：原生 TypeScript + WXML + WXSS，不使用 Taro/uni-app，也不做 H5 跳转。业务 API 与 Web/App/Desktop 共用 `packages/spacegate-admin-contract` 的类型和 `packages/spacegate-admin-shared` 的翻译基础文案。

## 功能范围

| 页面 | 路径 | 覆盖操作 |
| --- | --- | --- |
| 登录 | `pages/login/index` | 账号密码登录；公开配置启用 LDAP 时显示 LDAP 登录；中/英/日界面切换 |
| 概览 | `pages/overview/index` | 当前网关选择、网关/路由/插件实例数量、实例在线数、模块入口 |
| 网关 | `pages/gateways/index` | 网关列表/切换、详情 JSON 查看与编辑、新建、更新、删除 |
| 路由 | `pages/routes/index` | 网关内路由列表、完整规则 JSON 编辑、插件绑定 JSON、新建、更新、重命名、删除 |
| 插件 | `pages/plugins/index` | 插件实例列表/筛选、named/anon 实例增删改、展示名称、插件 Schema、已保存 Wasm Schema、Wasm 镜像 Schema 预览（含 HTTP 头/OCI 认证）、插件目录刷新 |
| AI 限流 | `pages/ai-queue/index` | 规则筛选、专用 tenant/model/path/policy 字段 + 扩展 JSON、增删改、规则解析、插件说明、规则 Schema |
| 实例 | `pages/instances/index` | 实例列表/健康状态、按网关/全局/路由重载 |
| 个人 | `pages/profile/index` | 当前用户/角色、显示名称/邮箱、修改密码、语言切换、退出登录 |

- access/refresh token **只保存在内存**，不写 `wx.storage`、不打印日志；小程序进程重启后必须重新登录。
- 写操作使用每次读取得到的 `version`；服务端返回 409 时适配器抛出 `ConflictError`（含 `serverVersion`），页面重新加载资源后由用户确认重试，不自动重试。
- 复杂嵌套规则/插件 spec 使用 JSON 文本域并在提交前校验；网关/路由/插件实例的 name、code、displayName、kind 等顶层字段使用专用输入控件。
- `401 -> 单次 refresh -> 原请求重试`；并发 401 共享同一次刷新。
- 响应必须是 `{ success: true, data }` 或 `{ success: false, error }`，否则视为非法响应。
- 文案：`miniprogram/i18n/` 是生成物，`messages/*.json` 是本地覆盖层；`zh-CN` 为默认语言。

## 在微信开发者工具中打开

1. 打开微信开发者工具 → 导入项目，目录选择 `spacegate-admin/apps/spacegate-admin-miniapp`（不是 `miniprogram/`）。
2. `project.config.json` 内的 `appid` 是占位值 `touristappid`。使用真实小程序时，把自己的 AppID 写入 **`project.private.config.json`**（已被 `.gitignore` 忽略），或直接改 `project.config.json`；不要把私有配置提交到仓库。
   - `project.private.config.json` 至少包含：
     ```json
     {
       "projectname": "spacegate-admin-miniapp",
       "appid": "wx0000000000000000",
       "setting": {
         "urlCheck": false
       }
     }
     ```
3. 工程已配置 `compileType: "miniprogram"`、`miniprogramRoot: "miniprogram/"`、`useCompilerPlugins: ["typescript"]`，开发者工具会直接编译 `.ts`。
4. 本地联调时关闭「不校验合法域名」限制（`urlCheck: false`）；真机必须把 API 域名加入小程序后台的 request 合法域名，并使用 HTTPS。

## 配置 API 基地址

默认地址为：

```text
http://127.0.0.1:3001/api/miniapp/v1
```

三种覆盖方式（优先级从高到低）：

1. 代码中调用 `configureApi({ baseUrl })`（`miniprogram/lib/api.ts`）。
2. 小程序 `ext.json` / 第三方平台 ext 配置：
   ```json
   {
     "extEnable": true,
     "extAppid": "wx0000000000000000",
     "ext": {
       "apiBaseUrl": "https://admin.example.com/api/miniapp/v1"
     }
   }
   ```
   适配器也接受 `apiBase` 或 `HAI_MINIAPP_API_BASE_URL` 键。
3. 未配置时使用上面的本地默认值。

API 基地址不是密钥，但生产必须使用 HTTPS；token 从不写入配置或 storage。

## 启动受限的小程序通道

主通道 `/api/v1` 继续强制 SM2 传输加密，微信无法实现。服务端为该端额外装配一个**独立** App：

- 前缀：`/api/miniapp/v1`
- 同一套 contract、procedures、IAM 会话与 `admin` 角色守卫
- 不启用 transport 加密（仅此通道）
- refresh token 只对同时携带以下头的请求走 JSON body：
  - `X-Request-Client: spacegate-admin-miniapp`
  - `X-HAI-Token-Transport: body`
- 所有 `/api/miniapp/v1/*` 业务请求必须带 `X-Request-Client: spacegate-admin-miniapp`，否则 403。
- 不暴露 OpenAPI/docs。

启动方式：

```bash
# 终端 1：主通道（加密）
pnpm --filter spacegate-admin-serv dev

# 终端 2：同一进程内再监听小程序通道（设置即启用）
HAI_SERV_MINIAPP_PORT=3001 pnpm --filter spacegate-admin-serv dev
```

`HAI_SERV_MINIAPP_PORT` 未设置时小程序通道完全不监听；端口非法会阻止启动。两个通道共用 `server.host`（`HAI_SERV_SERVER_HOST`，默认 `127.0.0.1`）。

### 安全边界（部署必读）

- **仅 HTTPS**：该通道没有传输加密，必须由同一受信 origin 的 HTTPS 反向代理终止 TLS 后转发，例如 `https://admin.example.com/api/miniapp/v1 -> http://127.0.0.1:3001/api/miniapp/v1`。
- **只允许内网/回环**：不要直接把 3001 暴露到公网；`server.host` 建议保持 `127.0.0.1` 或容器内网地址，反向代理只接受受信来源/网关转发。
- **仍有认证与授权**：登录、刷新、每个业务 procedure 都经过 IAM；所有 SpaceGate 管理接口要求 `admin` 角色，普通用户只能访问个人资料/会话接口。
- **可通过不设置 `HAI_SERV_MINIAPP_PORT` 完全关闭**；关闭后小程序无法登录。主通道不受影响。
- 主通道的 `closeModules` 仍只由主 `serv.listen` 拥有；小程序通道只关闭自身 HTTP 句柄，避免模块被重复释放。

## 开发命令

```bash
# 从仓库根执行
pnpm --filter spacegate-admin-miniapp sync-i18n
pnpm --filter spacegate-admin-miniapp typecheck
pnpm --filter spacegate-admin-miniapp test
```

- `sync-i18n`：复制 `packages/spacegate-admin-shared/messages/*.json` 到 `miniprogram/i18n/`，合并本地 `messages/*.json` 覆盖层，并生成 `miniprogram/i18n/messages.ts`。微信运行时不保证可 `require` JSON，因此运行时只 import 生成的 TS 模块；JSON 保留为可审查素材。
- `typecheck`：`tsc --noEmit`，使用本地 `types/wx.d.ts`，不依赖第三方小程序类型包。
- `test`：vitest + Node mock `wx`，不访问真实网络。覆盖适配器登录/鉴权头/单次刷新/409/登出、登录→概览→写入→409→重载→成功→登出流程，以及页面清单/项目配置/翻译回退。

## 原生验收限制

TypeScript 编译、页面清单和 mock `wx` 流程测试只能证明代码结构与企业务流程。**微信开发者工具编译、真机预览/发布包验收需单独执行并单独报告**；浏览器预览、H5 页面或 mock 测试都不能替代原生包验收。本轮未执行微信开发者工具/真机验收时，不得声称小程序已原生通过。

## 许可证

Apache-2.0
