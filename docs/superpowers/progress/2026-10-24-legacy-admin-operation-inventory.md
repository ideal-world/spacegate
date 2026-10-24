# SpaceGate 旧版 Vue 管理端逐页操作清单（迁移回归基线）

- 日期：2026-10-24
- 性质：只读静态盘点，未修改任何源码，未执行 git、构建或安装。
- 基线：`spacegate-admin-fe` 工作树 + 子模块 `spacegate-admin-fe/components/config`（`feature-wasm`，包含工作树中的未提交内容）。设计文档第 93 行明确要求以该工作树为准，不以外层同名仓库覆盖。
- 设计依据：`spacegate/docs/superpowers/specs/2026-10-15-spacegate-hai-framework-migration-design.md`（下称“设计文档”）。
- 用途：作为新 Hai Web/App/Desktop/miniapp 的逐项回归检查表。设计文档明确禁止用通用 JSON 文本框替代既有专用交互（设计文档 93 行）。
- 证据说明：所有非显然行为都给出 `文件:行号`。由于未运行浏览器，Element Plus 的焦点、动画、弹层挂载等纯运行时细节以代码语义为准，已在“遗留怪癖”中标注需要实测的项。

---

## 0. 结论速览

1. 旧管理端实际可路由页面只有 6 个：概览、网关、路由、插件、AI 队列限流、实例。登录不是独立页面，而是 401 时弹出的 `Login` 组件对话框。
2. 全部页面共享一个“当前网关”概念，来源是 URL query `gatewayName`，由顶栏 `ConsoleGatewaySelect` 维护；但“实例运维”页有自己独立的网关下拉，二者不同步。
3. 写操作全部经过 Rust admin-server 的 `X-Client-Version` 版本控制；**写成功响应不带 `X-Server-Version`**，所以旧前端依赖写后重新 GET 来同步版本。重载类 POST 也会推进服务端版本，这是旧实现的重要副作用。
4. 插件体系分两条完全不同的链路：
   - 原生插件：插件目录（`/plugin/list`、`/plugin/attr-*`）→ 插件实例配置（`/config/plugin*`）→ 资源绑定（`plugins[]`）。
   - Wasm 插件：插件中心定义（`code=wasm` 的 named 配置）→ 资源专属绑定配置（`binding_*` 字段）→ 网关/路由/规则/后端绑定。
5. 最容易被“通用 JSON 编辑器”误伤的专用表单是：网关监听器/TLS/Redis 连接池、HTTP 路由匹配/重写/后端、MCP 传输、插件 schema 表单、Wasm 定义抽屉、AI 租户限流规则编辑器、插件绑定优先级/拖拽排序。
6. 旧实现有若干真实缺陷（未保存关闭确认被“取消”绕过、路由规则级超时无 UI、schema enum 不渲染下拉、mono 插件自定义创建会写成 named、Wasm 绑定复制 `oci_auth`/`http_headers` 到每个绑定配置等），迁移时应“保留能力、修正实现”，而不是原样复制缺陷。

---

## 1. 应用骨架与全局交互

### 1.1 技术栈与入口

| 项 | 内容 | 证据 |
| --- | --- | --- |
| 框架 | Vue 3 + TypeScript + Vite + Element Plus + vue-router + vue-i18n | `spacegate-admin-fe/package.json:16-31` |
| 入口 | `createApp(App).use(El).use(router).use(SgAdm).use(i18n)` | `src/main.ts:34-77` |
| API 客户端 | `spacegate-admin-client`，`Api.setClient({ baseURL: '/api' })` | `src/main.ts:38-40` |
| 子模块组件 | `@components/config` 以 Vite alias 指向 `components/config/src` | `vite.config.ts:34-42` |
| 开发代理 | `/api` → `http://localhost:9992/` 并去掉 `/api` 前缀；`/ai-gateway` → `http://localhost:18080/` 并去掉 `/ai-gateway` | `vite.config.ts:10-28` |
| Monaco | 仅插件 JSON 编辑器使用，配置了 `json.worker` 与 `editor.worker` | `src/main.ts:12-33` |

### 1.2 路由表与导航入口

| 路径 | 页面组件 | 说明 | 证据 |
| --- | --- | --- | --- |
| `/` | 重定向 `/overview` | — | `src/router.ts:4` |
| `/overview` | `Overview.vue` | 概览 | `src/router.ts:5` |
| `/gateways` | `Gateways.vue` | 网关管理（列表 + 抽屉） | `src/router.ts:6` |
| `/gateway` | 重定向 `/gateways` | 旧入口；会丢掉 query | `src/router.ts:7` |
| `/routes` | `Routes.vue` | 路由管理（列表 + 抽屉） | `src/router.ts:8` |
| `/route` | 重定向 `/routes` | 旧入口；会丢掉 query | `src/router.ts:9` |
| `/plugins` | `Plugins.vue` | 插件中心 | `src/router.ts:10` |
| `/ai-queue` | `AiQueueRateLimit.vue` | AI 排队限流 | `src/router.ts:11` |
| `/instances` | `Instances.vue` | 实例运维 | `src/router.ts:12` |
| `/instance` | 重定向 `/instances` | 旧入口；会丢掉 query | `src/router.ts:13` |

- 没有 404/catch-all 路由，也没有路由级标题、鉴权守卫或 `meta`。
- 侧边栏菜单由 `App.vue` 内的 `pages` 数组硬编码：概览、网关、路由、插件、AI 限流、实例（`src/App.vue:31-38`）。
- 顶栏还有两个隐式入口：实例状态徽标点击进入 `/instances`；实例图标按钮也进入 `/instances`（`src/App.vue:137-139`、`src/App.vue:153`）。
- `Menu.vue`、`Header.vue`、`src/views/index.ts`、`src/views/Gateway.vue`、`src/views/Route.vue`、`src/views/Instance.vue`、`src/views/Plugin.vue` 均未被路由使用，属于历史/包装代码（`src/router.ts:1-14` 与 `src/views/index.ts:1-3` 对照）。

### 1.3 顶栏（全局壳）

| 操作 | 行为 | 证据 |
| --- | --- | --- |
| 折叠/展开侧栏 | `isMenuCollapse` 切换 | `src/App.vue:20-28`、`112-129` |
| 网关选择 | `ConsoleGatewaySelect` 双向绑定 `gatewayName` | `src/App.vue:130-133` |
| 实例健康徽标 | 每 5 秒拉取 `discoveryInstanceHealth`，显示在线/总数；0 个为 `info`，0 在线为 `danger`，全在线为 `success`，部分在线为 `warning` | `src/App.vue:50-55`、`78-98`、`137-139` |
| 语言切换 | 仅两项：中文 `zh-CN`、English `en-US`；直接改 `locale`，不持久化 | `src/App.vue:40-43`、`140-149` |
| GitHub | 新窗口打开 `https://github.com/ideal-world/spacegate` | `src/App.vue:150-152` |
| 实例入口 | 图标按钮跳 `/instances` | `src/App.vue:153` |

实例健康轮询失败时静默把计数置 0/0（`src/App.vue:78-93`），没有错误提示；这会让顶栏显示“实例 0/0”，容易与真实故障混淆。

### 1.4 `gatewayName` query 协议（跨页核心）

1. 顶栏 watch `route.query.gatewayName`，写入本地 `gatewayName`（`src/App.vue:57-63`）。
2. `gatewayName` 变化时把 query 写回当前路由（`src/App.vue:65-69`）。
3. 菜单跳转时把当前 `gatewayName` 带过去（`src/App.vue:71-76`）。
4. `ConsoleGatewaySelect` 在挂载时拉取 `/config/names`，若当前无值则自动选第一个（`src/components/ConsoleGatewaySelect.vue:37-48`），并 watch 空值再补第一个（`54-56`）。
5. 选择器内的“刷新”只重新拉网关名列表，不会清空已失效的选中值（`src/components/ConsoleGatewaySelect.vue:79-81`）。
6. “管理”按钮跳 `/gateways` 并携带当前值（`src/components/ConsoleGatewaySelect.vue:50-52`）；空列表时“去创建”也走同一入口（`72-77`）。

各页面读取方式不同：

- Overview：`route.query.gatewayName` 或 `gatewayNames[0]`（`src/views/Overview.vue:21-24`）。
- Routes：同样 fallback 到第一个网关（`src/views/Routes.vue:86-89`），并 watch 变化重新加载路由（`243`）。
- Plugins：只从 query 读，未选网关时为 `undefined`（`src/views/Plugins.vue:11`），Wasm 全局启用开关会提示先选网关。
- Instances：完全不读 query，自己维护 `selectedGateway` 并默认第一个（`src/views/Instances.vue:19`、`73-74`）。

### 1.5 登录、401、409 与错误呈现

| 场景 | 行为 | 证据 |
| --- | --- | --- |
| 登录方式 | 弹窗输入用户名/密码（实际是 `ak`/`sk`），`POST /auth/login` | `components/config/src/components/Login.vue:6-15` |
| JWT 传递 | 服务端 `Set-Cookie: jwt=...; path=/; HttpOnly; Max-Age=3600`；也接受 `Authorization: Bearer` | `spacegate/binary/admin-server/src/service/auth.rs:22-51`、`src/mw/authentication.rs:25-43` |
| 无 `--key` 的开发模式 | 不校验 JWT，登录也仍然成功但不发 Cookie | `auth.rs:31-46`、`authentication.rs:27-40` |
| 401 | SDK 抛 `ExceptionUnauthorized`；顶层拦截器弹不可关闭的登录框；登录后 `location.reload()` | `spacegate/sdk/admin-client/dist/api/index.js:75-85`、`src/main.ts:41-59` |
| 409 | SDK 抛 `ExceptionVersionConflict`；顶层显示硬编码英文 “version conflict, please refresh and commit” | `src/main.ts:63-66`、`dist/api/index.js:62-83` |
| 其他 HTTP 错误 | AxiosError 时直接 `ElMessage.warning(error.response.data)`（可能是对象/纯文本） | `src/main.ts:60-62` |
| 组件内错误 | 各页面自行 `catch` 后显示本地化消息；部分显示 `e.message` 原文 | 见各页面小节 |

关键版本语义（必须按真实协议迁移，设计文档 62 行）：

- 请求统一带 `X-Client-Version`，初值 `'0'`（`dist/api/index.js:55-61`）。
- GET 成功响应读 `X-Server-Version` 并更新客户端版本；POST/PUT/DELETE 成功响应**不返回**该头（`dist/api/index.js:62-74`；`binary/admin-server/src/mw/version_control.rs:32-57`）。
- 写操作成功后服务端 `version + 1`，但客户端版本仍旧；下一次写之前如果没有 GET，极可能 409。旧页面靠写后 `load()`/`loadRoutes()`/`refreshPluginInstancesList()` 的 GET 来同步。
- 409 响应带最新 `X-Server-Version`，但 SDK 在错误分支直接抛异常、没有更新客户端版本（`dist/api/index.js:75-83`），因此 409 后必须刷新页面（重新 GET）才能继续。
- `discovery reload` 的 POST 也被版本中间件当作写操作，会推进版本（`version_control.rs:59-65`），虽然它不改配置；这会造成“只点了几次重载，其他标签页就 409”。
- `/plugin/wasm/schema/preview` 和 `/plugin/refresh` 被显式排除在版本控制之外（`version_control.rs:63`、`71-90`）。

### 1.6 i18n 与主题

| 项 | 事实 | 证据 |
| --- | --- | --- |
| 默认语言 | `zh-CN` | `src/main.ts:16-20` |
| 消息表 | `zh-CN`、`en`、`es` 三份 | `src/locales/index.ts:5-10` |
| 切换器 | 只有 `zh-CN` 与 `en-US` | `src/App.vue:40-43` |
| `en-US` 与 `en` 的差异 | 消息键是 `en`，切换值是 `en-US`，依赖 vue-i18n 的地区回退；需在目标框架实测 | `src/locales/index.ts:5-10`、`src/App.vue:40-43` |
| `es` 状态 | 大量值仍是中文，且缺少 `menu.overview`、`menu.aiQueue`、`menu.instance`、`label.enableXRequestId`、`title.gatewayReload` 等键；切换器未提供 | `src/locales/es.ts:2-6`、`65-110` |
| 页面内文案 | 大多数页面用 `locale.startsWith('zh') ? 中文 : 英文` 的 `texts` computed，`es` 会落到英文 | 例：`src/views/Overview.vue:27-93` |
| 持久化 | 语言不写 localStorage/服务端 | `src/App.vue:40-43`、`main.ts:16-20` |
| 主题 | 没有主题切换、没有 dark class、没有 `prefers-color-scheme`；只有侧栏折叠 | `src/App.vue:105-161` 全文无主题逻辑 |
| 页面标题 | 没有 `document.title` 或路由 meta，浏览器标题固定为模板值 | `src/router.ts:1-14` |

### 1.7 共享 JSON 导入/导出与剪贴板

`components/config/src/utils/index.ts` 提供旧组件的通用能力：

| 函数 | 行为 | 证据 |
| --- | --- | --- |
| `saveJson(value, name, 'file'\|'clipboard')` | 文件下载 `${name}.json`；剪贴板写入并提示 “Copied to clipboard” | `utils/index.ts:100-123` |
| `fetchJson('file'\|'clipboard')` | 文件选择/读剪贴板，JSON.parse 后提示；Firefox 读剪贴板直接拒绝 | `utils/index.ts:125-184` |
| `downloadConfigItem` / `uploadConfigItem` | 整份 `ConfigItem` 下载/上传，上传前旧版 `SelectGateway` 有覆盖确认 | `utils/index.ts:61-98`、`components/SelectGateway.vue:103-113` |
| `catchAdminServerError` | 409 显示英文版本冲突；其他显示 `(status): data` | `utils/index.ts:25-40` |
| `unwrapResponse` | 非 2xx 或非 JSON content-type 时抛 `ResponseError` | `utils/index.ts:42-59` |
| `hashColor` | 由插件 code/标题生成浅色背景 | `utils/index.ts:186-220` |
| `keyPluginId` | `code-n-name` / `code-a-uid` / `code-m` | `utils/index.ts:222-231` |

注意：活动页面里只有网关表单和路由抽屉的“高级 JSON”使用这些能力；路由抽屉的 JSON 只在文本框与剪贴板之间复制，不提供文件下载（`src/components/RouteEditorDrawer.vue:1116-1129`）。

---

## 2. 页面与操作清单

### 2.1 概览 `/overview`

#### 2.1.1 路由与入口

- 路由：`/overview`（`src/router.ts:5`）；侧栏“概览”（`src/App.vue:32`）。
- 数据依赖当前 `gatewayName`（query 或第一个网关）。

#### 2.1.2 用户可见操作

| 操作 | 触发 | 行为 | API | 证据 |
| --- | --- | --- | --- | --- |
| 查看指标 | 进入页面 | 并行加载网关名、全部插件配置数、实例列表/健康，然后加载当前网关详情 | `getConfigNames`、`getConfigPluginAll`、`discoveryInstanceList`、`discoveryInstanceHealth`、`getConfigItemGateway`、`getConfigItemRouteNames`、逐条 `getConfigItemRoute` | `src/views/Overview.vue:107-170` |
| 在线实例卡片 | 点击 | 跳 `/instances`，保留 `gatewayName` | — | `174-188`、`165-167` |
| 网关卡片 | 点击 | 跳 `/gateways` | — | `189-193` |
| 路由卡片 | 点击 | 跳 `/routes` | — | `194-198` |
| 插件配置卡片 | 点击 | 跳 `/plugins` | — | `199-203` |
| 配置路由 | 按钮 | 跳 `/routes` | — | `180` |
| 管理网关 | 按钮 | 跳 `/gateways` | — | `213` |
| 运维 | 按钮 | 跳 `/instances` | — | `229` |
| 查看全部路由 | 按钮 | 跳 `/routes` | — | `248` |
| 查看插件 | 按钮 | 跳 `/plugins` | — | `275` |
| 实例健康表 | 只读 | 显示实例 ID 与在线/离线标签 | 同加载 | `231-238` |
| 路由概况表 | 只读 | 只显示前 6 条：名称、类型、匹配摘要、后端摘要、优先级（MCP 显示 `-`） | 同加载 | `250-266`、`102-105` |

#### 2.1.3 指标定义（易误解）

| 指标 | 真实含义 | 证据 |
| --- | --- | --- |
| 在线实例 | `healthy === true` 的实例数 / 实例总数 | `src/views/Overview.vue:95`、`183-188` |
| 网关 | `getConfigNames()` 返回的配置项数量（不是运行时网关对象去重后的数量） | `119-126`、`189-193` |
| 路由 | **当前网关**下已加载成功的路由数量 | `148-163`、`194-198` |
| 插件配置 | `getConfigPluginAll()` 的全部条目，包括 Wasm 绑定配置，不是“插件类型数” | `128-135`、`199-203` |
| 当前网关面板 | 监听摘要、网关级插件数量、`redis_url` | `215-219` |
| 路由概况 | 当前网关路由，按后端返回顺序截取前 6 条 | `250` |

#### 2.1.4 状态与边界

- 加载失败：`load()` 捕获任一并行请求失败，显示一次 “概览数据加载失败…”；由于 `loadGatewayDetail()` 在同一个 try 内且位于 `Promise.all` 之后，任一指标请求失败时它会被跳过（`107-117`、`148-163`）。`loadGatewayDetail()` 自身失败时静默清空。
- 网关列表为空：`gatewayName` 为 `undefined`，当前网关面板显示空态，路由表为空（`148-153`、`220`）。
- 当前网关详情失败：面板变为空态、路由清空，不额外提示（`159-162`）。
- 切网关：只 watch `route.query.gatewayName` 重新加载“当前网关详情 + 路由”；指标（网关数/插件数/实例）不会重新加载（`169`）。
- 无手动刷新按钮；只能刷新浏览器或切网关。
- 实例健康表无空态组件，直接空表（`231-238`）。

#### 2.1.5 跨页依赖

- 依赖顶栏 `gatewayName`。
- 所有卡片都是带 query 的跳转。
- “AI 网关治理待完善”是静态说明，提示模型目录/Key/Spend/Logs 暂无后端 API（`269-277`、`57-59`）。

---

### 2.2 网关管理 `/gateways`

#### 2.2.1 路由与入口

- 路由 `/gateways`（`src/router.ts:6`），侧栏“网关”（`src/App.vue:33`）。
- 顶栏“管理网关”和空选择器的“去创建”都进入此页（`src/components/ConsoleGatewaySelect.vue:50-52`、`72-77`）。

#### 2.2.2 操作清单

| 操作 | 触发 | 行为/确认 | API | 证据 |
| --- | --- | --- | --- | --- |
| 加载列表 | 进入页面 | 先 `GET /config/names`，再并行 `GET /config/item/{configItemName}/gateway`；同时并行统计每个配置项的路由数 | `getConfigNames`、`getConfigItemGateway`、`getConfigItemRouteNames` | `src/views/Gateways.vue:119-145` |
| 刷新 | 右上“刷新” | 重新执行加载；无二次确认 | 同上 | `239` |
| 新建网关 | 右上或空态按钮 | 打开抽屉，初始值 `new-gateway` + 一个 `http` 监听 `9000` | — | `147-153`、`83-117` |
| 编辑网关 | 行内“编辑” | 深拷贝网关，记录原配置项 ID；抽屉标题显示名称 | — | `155-161` |
| 查看路由 | 行内“路由” | 跳 `/routes?gatewayName=<配置项ID>` | — | `198-200` |
| 删除网关 | 行内“删除” | 弹确认，消息含名称与“该网关下有 N 条路由将一并删除”；确认按钮为危险色 | `DELETE /config/item/{configItemName}/gateway` | `182-196` |
| 保存（新建） | 抽屉“保存” | 仅校验名称非空；`POST /config/item/{name}/gateway` | `postConfigItemGateway` | `163-180` |
| 保存（编辑） | 抽屉“保存” | 名称只读；`PUT /config/item/{原配置项ID}/gateway` | `putConfigItemGateway` | `163-180`、`GatewayForm.vue:124-126` |
| 取消 | 抽屉“取消” | 直接 `drawerOpen=false`；**不会触发未保存确认** | — | `285`、`216-223` |
| 关闭抽屉（X/遮罩/ESC） | `before-close` | 若表单 JSON 快照变化，弹“有未保存的修改，确认放弃并关闭？”（标题为空） | — | `209-223` |
| 空态 | 无网关 | `el-empty` + “新建网关” | — | `268-272` |

#### 2.2.3 列表列

| 列 | 内容 | 证据 |
| --- | --- | --- |
| 名称 | `gateway.name` | `246` |
| 监听 | `name:port/protocol` 逗号拼接，无监听为 `-` | `247-249`、`src/views/console-utils.ts:3-8` |
| 路由 | 该配置项下路由名数量；失败按 0，未取到显示 `-` | `250-252`、`133-137` |
| 插件 | `plugins.length` | `253-255`、`console-utils.ts:10-12` |
| Redis | `parameters.redis_url || '-'` | `256-258` |
| 操作 | 路由 / 编辑 / 删除 | `259-267` |

#### 2.2.4 网关表单字段（`GatewayForm.vue`）

| 字段 | 类型 | 必填/可选 | 默认值 | 校验/单位/枚举 | 证据 |
| --- | --- | --- | --- | --- | --- |
| `name` | string | 必填 | `new-gateway`；旧选择器默认 `New-Gateway` | 仅 trim 非空；**编辑模式只读** | `Gateways.vue:164-167`、`GatewayForm.vue:124-126`、`SelectGateway.vue:17-19` |
| `parameters.redis_url` | string\|null | 可选（开关） | `null` | 开启后可填 `redis://...` | `GatewayForm.vue:137-146`、`Gateways.vue:87` |
| `redis_pool_max_size` | number\|null | 可选（开关） | 开关开启默认 `32` | ≥1，整数，单位“连接数”；仅当 `redis_url` 有值时显示 | `147-157` |
| `redis_pool_wait_timeout_ms` | number\|null | 可选（开关） | `1000` | ≥1，步进 100，单位 ms | `158-167` |
| `redis_pool_create_timeout_ms` | number\|null | 可选（开关） | `1000` | ≥1，步进 100，单位 ms | `168-177` |
| `redis_pool_recycle_timeout_ms` | number\|null | 可选（开关） | `1000` | ≥1，步进 100，单位 ms | `178-187` |
| `log_level` | string\|null | 可选（开关） | `info` | `trace/debug/info/warn/error` | `189-204` |
| `lang` | string\|null | 可选（开关） | `zh-CN` | `zh-CN/en` | `205-217` |
| `enable_x_request_id` | boolean | 是 | `false` | 开关，无“可选”语义 | `218-223`、`Gateways.vue:94` |
| `ignore_tls_verification` | boolean\|null | 模型可空 | `null`；开关操作后写入 `false/true` | 开关；`null` 与 `false` 在 UI 上都显示“未启用” | `224-229`、`Gateways.vue:95` |
| `observability` | object | 模型必填 | 见下 | **表单无任何控件**；只能通过 JSON 导入/导出保留 | `Gateways.vue:96-104`、`GatewayForm.vue:317-476` 无 observability |
| `listeners[]` | array | 至少建议 1 个，但无保存校验 | 新建网关：`http:9000`；表单“添加监听”默认 `new listener:8080` | 见 2.2.5 | `Gateways.vue:106-114`、`GatewayForm.vue:22-32` |
| `plugins[]` | `PluginBinding[]` | 可选 | `[]` | 通过 `PluginListForm`，见 2.4.6 | `GatewayForm.vue:264-268` |

`observability` 默认对象（新建时）：`enabled:false`、`service_name:'spacegate'`、`otlp_endpoint:'http://localhost:4317'`、`protocol:'grpc'`、`traces.enabled:false/sample_ratio:1`、`metrics.enabled:false/export_interval_ms:60000`、`logs.enabled:false/level:'info'`（`src/views/Gateways.vue:96-104`；旧选择器同值但 `60000n`，`SelectGateway.vue:29-46`）。

#### 2.2.5 监听器字段（`ListenerForm.vue`）

| 字段 | 类型 | 默认 | 校验/枚举 | 证据 |
| --- | --- | --- | --- | --- |
| `name` | string | 新建监听无默认值（新建网关的监听是 `http`） | 无唯一性校验 | `GatewayForm.vue:106-113`、`ListenerForm.vue:53-55` |
| `ip` | string\|null | `null`（显示“默认 IPv6”）；开关默认 `0.0.0.0` | 自由文本 | `ListenerForm.vue:56-65` |
| `port` | number | `8080`（新建网关里的监听是 `9000`） | `0..65535`，整数，必填 | `ListenerForm.vue:66-68`、`constants/index.ts:4-16` |
| `protocol.type` | `http`\|`https` | `http` | 切换时重建整个 protocol 对象 | `ListenerForm.vue:29-48`、`69-74` |
| `hostname` | string\|null | `null`（显示“任意主机”）；开关默认 `localhost` | 自由文本 | `ListenerForm.vue:75-84` |
| TLS `mode` | `Terminate`\|`Passthrough` | `Terminate` | 仅 https 显示 | `ListenerForm.vue:85-92`、`29-47` |
| TLS `key` | string（textarea） | `""` | 无校验 | 同上 |
| TLS `cert` | string（textarea） | `""` | 无校验 | 同上 |
| TLS `http2` | boolean\|null | `false`（新建 https 时） | **没有 UI 控件**，值随 JSON 保留 | `ListenerForm.vue:40-45` |

监听器切换 http→https 会把 `protocol` 整个替换为新的 TLS 默认对象；https→http 会丢掉 TLS 配置（`ListenerForm.vue:29-48`）。

#### 2.2.6 高级操作（JSON 导入/导出）

| 操作 | 行为 | 证据 |
| --- | --- | --- |
| 上传 → 剪贴板/文件 | 读取 JSON；**编辑模式强制保留当前 `name`**；创建模式整体替换 | `GatewayForm.vue:42-53` |
| 下载 → 剪贴板/文件 | 下载 `${name}.json` / 写剪贴板 | `GatewayForm.vue:38-40`、`utils/index.ts:100-123` |
| 弹层 | 先点“上传/下载”再选来源/目标，popover 受控 | `GatewayForm.vue:278-309` |

上传只做 `JSON.parse`，没有结构校验（`utils/index.ts:141-149`）；导入后 `listeners/plugins` 可能为 `undefined`，后续 `modelValue.listeners.length` 会抛错（`GatewayForm.vue:242`），需要实测。

#### 2.2.7 状态与边界

- 加载失败：清空列表并提示“网关列表加载失败，请确认 admin-server 已启动”（`119-144`）。
- N+1：每个配置项一次网关 GET，再一次路由名 GET；网关多时首屏慢（`122-137`）。
- 配置项 ID 与运行时名称分离：`configItemNames[gateway.name] = configItemName`；编辑保存/删除/跳路由都用配置项 ID，展示用 `gateway.name`（`127-132`、`203-205`）。
- 编辑时名称只读，注释却说“允许同时修改运行时网关名称”（`src/views/Gateways.vue:22` 与 `GatewayForm.vue:125` 矛盾）；由于没有网关重命名 API，改名只能用 JSON 导入且会被强制保留原名。
- 删除网关的级联语义**取决于后端**：
  - 文件后端删除整个网关目录（其中包含 routes 目录），因此路由会一起消失（`spacegate/crates/config/src/service/fs/delete.rs:18-28`）。
  - 通用 `delete_config_item` 也会先删网关再删全部路由（`spacegate/crates/config/src/service.rs:132-141`）。
  - K8s 后端的 `delete_config_item_gateway` 只删除 `Gateway`（以及关联 Secret/插件目标），**不会删除该网关下的 Route 资源**（`spacegate/crates/config/src/service/k8s/delete.rs:17-36`）。
  - 而 UI 的确认文案固定承诺“该网关下有 N 条路由将一并删除”（`src/views/Gateways.vue:45`、`185-186`）。迁移时必须按实际后端明确行为，不能继续用同一句话覆盖两种语义。
- 保存失败只显示“网关保存失败”，不展示后端文本（`177-179`）。
- 删除取消通过字符串 `'cancel'/'close'` 判断，Element Plus 版本差异下可能失效（`192-195`）。

#### 2.2.8 跨页依赖

- 保存/删除后重新 `load()`，影响顶栏网关选项（但顶栏不会自动刷新）。
- 文件后端删除网关会连带删除路由（K8s 后端不一定）；路由页若停留在该网关，下一次加载会得到空列表或错误。
- 网关级插件绑定直接影响插件中心 Wasm 卡片的“当前网关启用”状态（见 2.4.3）。

---

### 2.3 路由管理 `/routes`

#### 2.3.1 路由与入口

- 路由 `/routes`（`src/router.ts:8`），侧栏“路由”（`src/App.vue:34`）。
- 概览的“配置路由/路由卡片/查看全部”、网关行内“路由”都进入此页（`Overview.vue:180/194/248`、`Gateways.vue:198-200`）。

#### 2.3.2 操作清单

| 操作 | 触发 | 行为/确认 | API | 证据 |
| --- | --- | --- | --- | --- |
| 加载网关名 | 进入页面 | `GET /config/names`；无 query 时自动写入第一个网关 | `getConfigNames` | `src/views/Routes.vue:150-161` |
| 加载路由 | 进入/切网关 | `GET /config/item/{gw}/route/names` 后逐条 `GET .../route/item/{name}`；过滤 null | `getConfigItemRouteNames`、`getConfigItemRoute` | `163-179` |
| 搜索 | 输入 | 前端按 `route_name`、主机名摘要、后端摘要做小写包含匹配 | — | `91-101`、`265` |
| 新建路由 | 右上/空态 | 打开抽屉，默认 HTTPRoute：`new-route`、`/` Prefix、`127.0.0.1:80`、权重 1 | — | `181-186`、`113-148` |
| 编辑路由 | 行内“编辑” | 深拷贝；记录 `originalRouteName` | — | `188-193` |
| 复制路由 | 行内“复制” | 深拷贝后名称加 `-copy`，以“新建”模式打开 | — | `195-201` |
| 删除路由 | 行内“删除” | 确认“确认删除路由 X？”；危险按钮 | `DELETE /config/item/{gw}/route/item/{name}` | `228-241` |
| 保存（新建） | 抽屉“保存” | 名称非空、结构校验后 `POST` | `postConfigItemRoute` | `203-226` |
| 保存（改名） | 抽屉“保存” | 当 `originalRouteName !== route_name` 时走 rename API，提交完整配置 | `POST .../route/item/{old}/rename` | `213-214` |
| 保存（普通编辑） | 抽屉“保存” | `PUT .../route/item/{originalName}` | `putConfigItemRoute` | `215-217` |
| 取消 | 抽屉“取消” | 直接关闭；**绕过未保存确认** | — | `1134`、`655-661` |
| 关闭抽屉（X/遮罩/ESC） | `before-close` | 有改动则确认放弃 | — | `655-661` |
| 空态 | 无路由 | 空态 + “新建路由”（无网关时禁用） | — | `298-302` |
| 刷新 | — | **没有刷新按钮**；依赖切网关/重进页面 | — | `253-261` 只有“新建路由” |

#### 2.3.3 列表列

| 列 | 内容 | 证据 |
| --- | --- | --- |
| 路由名称 | `route_name` | `268` |
| 类型 | MCPRoute / HTTPRoute | `269-272`、`console-utils.ts:14-20` |
| 主机名 | 空数组显示 `*`，否则逗号拼接 | `274-276`、`console-utils.ts:69-72` |
| 匹配 | MCP：`transport path` 或 `sse + message`；HTTP：首条规则首条匹配；无规则 `match all`；多条规则追加 `+N more rules` | `277-279`、`console-utils.ts:22-53` |
| 后端 | 首个后端 Host/K8s/File 摘要；多个追加 `+N more`；空 `-` | `280-282`、`console-utils.ts:55-67` |
| 优先级 | MCP 显示 `-`，HTTP 显示数值 | `283-285`、`108-111` |
| 插件 | 路由级 `plugins.length`（不含规则/后端插件） | `286-288`、`10-12` |
| 操作 | 编辑/复制/删除 | `289-297` |

#### 2.3.4 路由抽屉结构

抽屉宽度 78%，五个页签（`src/components/RouteEditorDrawer.vue:675`、`701-1130`）：

| 页签 | 内容 | 证据 |
| --- | --- | --- |
| 基础信息 | 路由类型（仅创建时可切）、名称、优先级、hostnames | `702-745` |
| 传输（仅 MCP） | transport、path 或 SSE/消息路径、timeout_mode、session_affinity | `747-795` |
| 匹配条件（仅 HTTP） | 规则折叠面板；每条规则可“匹配全部”或配置多条 match | `797-918` |
| 目标后端 | MCP：扁平后端列表；HTTP：按规则分组的后端列表 | `920-1087` |
| 插件链 | 路由级插件 + 每条规则的插件（MCP 只有路由级） | `1089-1114` |
| 高级 JSON | 复制、读取剪贴板、应用 JSON、20 行 textarea | `1116-1129` |

顶部固定提示“保存后配置写入 admin-server；如需生效，请在实例运维中触发网关重载”（`687-692`、`53`）。

#### 2.3.5 路由表单字段

#### 基础与 HTTP 匹配

| 字段 | 类型 | 必填/默认 | 校验/枚举/单位 | 证据 |
| --- | --- | --- | --- | --- |
| 路由类型 | `http`\|`mcp` | 创建时默认 `http`；编辑不可切 | 切换会**整体重置**当前模型（hostnames/plugins/规则全丢） | `273-282`、`709-714` |
| `route_name` | string | 必填，默认 `new-route` / `mcp-route` | trim 非空 | `313-326`、`565-567` |
| `priority` | number | 默认 0 | `-5000..5000`；MCP 无此字段 | `721-725`、`307-311` |
| `hostnames` | string[]\|null | 默认 `null`（匹配所有） | 开关开启写入 `['example.com']`；关闭写 `null`；元素可增删 | `284-289`、`406-414`、`727-742` |
| 规则 `matches` | array\|null | 每条规则默认一条 `Prefix /` | `null` 表示匹配全部 | `291-298`、`338-347`、`432-451` |
| 路径 `kind` | `Prefix`\|`Exact`\|`RegExp` | `Prefix` | 必填；值必须以 `/` 开头 | `828-840`、`583-586` |
| 路径 `value` | string | `/` | 必须以 `/` 开头 | 同上 |
| 路径 `replace` | string\|null | `null` | 空字符串在打开时归一为 `null`；非空必须以 `/` 开头 | `358-362`、`841-845`、`585` |
| `method` | string[]\|null | `null` | 多选：GET/POST/PUT/DELETE/PATCH/HEAD/OPTIONS（**无 TRACE/CONNECT**） | `847-851` |
| `header[]` | array\|null | `null` | 每条 `kind: exact\|reg_exp`、`name`、`value` 或 `re`、`replace`；空数组归一 `null` | `453-461`、`871-883` |
| `query[]` | array\|null | `null` | 每条 `kind: exact\|regular`、`value.key`、`value.value` 或 `value.re`；无 replace 字段 | `463-471`、`895-906` |
| 规则 `timeout_ms` | number\|null | `null` | **无 UI**，只在模型/JSON 中保留 | `343-345`、`920-1087` 无该控件 |
| 规则 `timeout_mode` | `request`\|`disabled`\|null | `null` | **无 UI** | 同上 |
| 规则 `balance_policy` | `random`\|`ip_hash`\|`mcp_session`\|null | `null` | **无 UI** | 同上 |

#### 后端（HTTP 与 MCP 共用字段）

| 字段 | 类型 | 默认 | 校验/枚举/单位 | 证据 |
| --- | --- | --- | --- | --- |
| `host.kind` | `Host`\|`K8sService`\|`File` | `Host` | 切换时重置 host 并调整 port | `495-508`、`933-968` |
| Host `host` | string | `127.0.0.1` | 自由文本 | `942-946` |
| K8s `name` / `namespace` | string | `service-name` / `default` | 自由文本 | `947-958` |
| File `path` | string | `/var/www/html` | 自由文本；port 置 `null` | `959-968`、`504-507` |
| `port` | number\|null | `80` | `1..65535`（File 不显示也不校验） | `964-968`、`576` |
| `protocol` | `http`\|`https`\|null | `http` | 可清空为 null（表示默认） | `970-978` |
| `weight` | number\|null | `1` | ≥0；null 时归一为 1 | `979-983`、`367-368`、`577` |
| `timeout_ms` | number\|null | `null` | ≥0；占位符“继承”；单位 ms | `984-988` |
| `downgrade_http2` | boolean\|null | `false` | 开关 | `989-993`、`308` |
| `plugins[]` | binding | `[]` | 每个后端独立插件链 | `995-1001`、`1075-1081` |

#### MCP 专属

| 字段 | 类型 | 默认 | 校验/枚举 | 证据 |
| --- | --- | --- | --- | --- |
| `transport` | `streamable_http`\|`legacy_sse` | `streamable_http` | 切换时补齐/清空 `legacy_sse` | `323-336`、`380-388`、`667-671` |
| `path` | string | `/mcp` | 必须以 `/` 开头 | `760-762`、`571-573` |
| `legacy_sse.sse_path` | string | `/sse` | 必须以 `/` 开头 | `763-774`、`568-570` |
| `legacy_sse.message_path` | string | `/message` | 必须以 `/` 开头 | 同上 |
| `timeout_mode` | `disabled`\|`request` | `disabled` | 下拉 | `777-783` |
| `session_affinity` | `mcp_session`\|`none` | `mcp_session` | 下拉 | `785-791` |
| `backends[]` | array | 一个 `Host 127.0.0.1:80`，timeout_mode `disabled` | **至少一个**，删除最后一个会警告 | `374-395`、`486-493`、`574` |

#### 2.3.6 快速示例（仅 HTTP）

| 按钮 | 结果 | 证据 |
| --- | --- | --- |
| API 路由 | 名称保留（非 `new-route` 时）；`/api/` Prefix 重写 `/`；`127.0.0.1:8080`；priority 100；rule timeout 5000ms | `510-528` |
| 静态文件 | `/` Prefix；File 后端 `/var/www/spacegate-admin`；port null；priority 0 | `529-544` |
| K8s 服务 | hostnames `api.example.com`；`service-name.default:80`；priority 50；timeout 5000ms | `545-560` |

示例会整体替换模型（包括已有插件），没有确认。

#### 2.3.7 JSON 高级编辑

- “复制 JSON”：先把当前模型序列化进 `jsonText`，再写剪贴板；失败提示浏览器不允许（`605-627`）。
- “读取剪贴板”：读取文本到 `jsonText`，不自动解析（`629-635`）。
- “应用 JSON”：`JSON.parse` 后整体替换模型，再 `normalizeRoute()`；只捕获解析错误，**没有结构校验**（`609-617`）；页面文案“应用前会做基础结构校验”（`137`）与实际不符。
- 切到“高级 JSON”页签时自动同步当前模型（`663-665`）。
- 保存时校验依赖当前模型，不依赖 `jsonText`；用户修改文本但未点“应用”时，保存的是旧模型（`requestSave` 595-603）。
- 这里的编辑器是 `el-input type=textarea`，不是 Monaco（`1127`），对小程序友好；但插件 JSON 使用 Monaco。

#### 2.3.8 保存校验（`validateRoute`）

| 校验 | 规则 | 证据 |
| --- | --- | --- |
| 名称 | 非空 | `565-567` |
| MCP 路径 | streamable：`path` 以 `/` 开头；legacy：`sse_path`/`message_path` 以 `/` 开头 | `567-573` |
| MCP 后端 | 至少一个 | `574` |
| MCP 后端端口 | 非 File 时必须 1..65535 | `575-577` |
| MCP 权重 | 非空时 ≥0 | `577` |
| HTTP 规则 | 至少一条 | `581` |
| HTTP 路径/重写 | 值、replace 均需 `/` 开头 | `582-586` |
| HTTP 后端端口/权重 | 同 MCP | `587-590` |
| **未校验** | hostnames 格式、priority 范围、插件重复/优先級、HTTP 后端数量、header/query 完整性 | 全文 `565-593` |

#### 2.3.9 状态与边界

- 没有网关：创建按钮禁用，空态按钮也禁用（`259`、`300`）。
- 路由加载失败：清空并提示（`173-178`）。
- 保存失败：统一“路由保存失败”，不展示后端详情（`221-223`）。
- 创建重名：没有前端查重；后端 `create_config_item_route` 在文件存在时返回 `"route existed"`，服务层包装成 500 文本（`spacegate/crates/config/src/service/fs/create.rs:37-46`、`admin-server/src/error.rs:14-19`），前端只显示通用失败。
- 复制重名：同样会失败，不会覆盖（旧子模块页面曾提供“Overwrite”确认，但活动页面没有；`components/config/src/views/Routes.vue:154-166` 对比 `src/views/Routes.vue:195-201`）。
- 改名：后端要求新旧名不同、payload 的 `route_name` 等于新名、旧路由存在、新名不存在；先创建新路由再删除旧路由，**不是原子操作**（`spacegate/crates/config/src/service.rs:145-162`）。若删除失败会同时存在两份。
- 改名/删除不会清理 Wasm 绑定配置中的 `binding_owner`/名称引用，可能留下孤儿绑定配置（绑定配置命名见 2.4.5）。
- 保存后不自动重载网关；只有提示文案（`53`、`687-692`）。
- 规则级 `timeout_ms`/`timeout_mode`/`balance_policy` 只能通过 JSON 页签编辑，普通表单无法发现它们（`920-1087` 无控件）。

#### 2.3.10 跨页依赖

- 依赖顶栏网关选择；网关删除后此页路由可能为空。
- 路由插件绑定引用插件实例；插件实例删除后，路由仍保留引用（加载时会显示“实例不存在”警告，见 2.4.7）。
- 路由后端插件、规则插件、路由插件三层都影响运行时；保存后需要实例重载。
- 路由列表的“插件”列只统计路由级插件，容易误解（`286-288`）。

---

### 2.4 插件中心 `/plugins`

#### 2.4.1 路由与入口

- 路由 `/plugins`（`src/router.ts:10`），侧栏“插件”（`src/App.vue:35`）。
- 概览“插件配置卡片/查看插件”进入此页（`Overview.vue:199-203`、`275`）。

#### 2.4.2 页面级操作

| 操作 | 行为 | 证据 |
| --- | --- | --- |
| 刷新配置 | 通过 `:key="panelKey"` 强制重建 `PluginPanel`；会丢失当前选中插件、搜索、详情抽屉 | `src/views/Plugins.vue:10`、`26-28`、`48` |
| 原生插件 / Wasm 扩展页签 | `activeTab` 切换；切换时清空搜索 | `PluginPanel.vue:64`、`620-623`、`460-462` |
| 搜索 | 原生按 code/描述；Wasm 按标题/描述；清空恢复 | `625-630`、`266-284` |
| 原生插件卡片 | 图标、`meta.title`（回退 code）、code、版本、描述、主页/仓库菜单、底部“配置” | `643-680`、`296-298` |
| Wasm 卡片 | 标题、描述、启用开关、编辑、删除 | `716-755` |
| 添加自定义插件 | 打开 `ThirdPartyWasmDrawer`（新建） | `712-714`、`407-410` |
| 重载提示 | 保存/删除/启停/来源同步后显示警告条，带“前往实例运维” | `690-704`、`153`、`612-614` |

`PluginPanel` 的原生插件数据来源：

1. 先 `GET /plugin/attr-all`。
2. 失败则降级为 `GET /plugin/list` + 逐个 `GET /plugin/attr/{code}`。
3. 完成后加载 Wasm 实例与当前网关；`pluginsLoading` 在此期间为 true（`PluginPanel.vue:333-351`）。
4. `nativePluginAttrs` 过滤掉 code 以 `wasm` 开头的插件（`155-157`、`isWasmPluginCode` `wasmPlugin.ts:88-91`）。

#### 2.4.3 原生插件实例管理（选中某个插件后）

| 操作 | 行为/确认 | API | 证据 |
| --- | --- | --- | --- |
| 返回 | 清空 code/attr/instances，回到卡片列表 | — | `427-431`、`769` |
| 搜索实例 | 只匹配 named 实例；按实例名或展示名包含 | — | `440-445`、`793` |
| 创建实例 | 打开抽屉；mono 插件默认 `kind=mono`，否则 `kind=named`、名称 `new-instance` | — | `464-486` |
| 编辑实例 | 深拷贝 spec 后打开抽屉 | — | `516-521` |
| 保存实例 | 先 `PluginForm.validate()`；有必填缺失则中断；创建 `POST /config/plugin`，更新 `PUT /config/plugin`；成功后关闭并刷新当前 code 的实例列表 | `postConfigPlugin`、`putConfigPlugin` | `487-582` |
| 删除实例 | 确认“删除后已引用该配置的资源可能无法正常加载插件”；确认按钮删除色 | `DELETE /config/plugin?code&kind&name` | `522-539` |
| 空态 | “暂无插件配置” | — | `814` |

Mono 插件的特殊行为：详情页只显示元数据，不显示实例列表或创建按钮（`789-791`）；因此**旧 UI 无法在插件中心编辑 mono 实例**。

#### 2.4.4 Wasm 扩展卡片（插件中心定义）

- 只展示 `code` 属于 Wasm、`kind=named`、`spec.binding_scope === undefined`、且 `spec.plugin_name !== 'ai-gateway-queue'` 的配置（`wasmPlugin.ts:97-103`、`PluginPanel.vue:252-264`）。
- 排序：`spec.priority` 降序，再按展示名/标题/插件名/name/code 自然排序（`wasmPlugin.ts:105-127`）。
- 标题优先级：顶层 `display_name` → `spec.display_name` → `spec.plugin_name` → named `name` → `wasm`（`PluginPanel.vue:163-169`）。
- 描述优先级：`spec.description` → `spec.url` → i18n “Wasm 插件实例”（`171-175`、`260`）。
- 全局启用开关：
  1. 必须已选网关，否则提示“请先在顶部选择网关”。
  2. 读取当前网关；若未加载则重试一次。
  3. 用 `setPluginInstanceRefEnabled` 在网关 `plugins[]` 中增删该实例引用；新增项 `priority: 0`，已有项缺失 priority 时补 0。
  4. `PUT /config/item/{gateway}/gateway`。
  5. 成功后显示重载提示，但**不自动重载**。
  6. 失败直接显示 `e.message`。
  证据：`PluginPanel.vue:361-405`、`wasmPlugin.ts:187-202`。
- 编辑/删除：复用 `ThirdPartyWasmDrawer`；删除后设置重载提示、刷新列表并 `emit('changed')`（`412-425`、`541-560`）。

#### 2.4.5 原生插件表单 `PluginForm.vue`

| 字段/区域 | 行为 | 证据 |
| --- | --- | --- |
| 展示名称 `display_name` | 顶层字段，空字符串也保存为 `''`（由上层转 null） | `25-30`、`207-209` |
| 实例名称 `name` | 仅 `kind=named` 显示；必填（由 Schema 校验/后端校验） | `210-213` |
| `uid` | 仅 `kind=anon` 显示，禁用 | `214-216` |
| mono 提示 | `kind=mono` 显示“配置会以插件类型为唯一实例保存” | `217-223` |
| Schema 加载 | 监听 `code`，`GET /plugin/schema/{code}`；失败置 null | `65-76` |
| 表单/JSON 切换 | 有非空 schema 才显示“表单”页签；无 schema 强制 JSON | `78-93`、`238-254` |
| 默认值填充 | schema 有默认值时、且 spec 为空，调用 `SchemaForm.initDefaults` | `88-92` |
| JSON 示例 | 从 schema 的 `default`/`example`/`examples` 递归生成；可复制或填入编辑器 | `155-202`、`263-266` |
| 编辑器 | Monaco JSON，schema URI 绑定插件 code；JSON 解析失败时切回 JSON 模式并报错 | `95-128`、`hooks/index.ts:30-112` |
| 暴露给父组件 | `getJson()`：表单模式返回对象，否则 Monaco 解析；`validate()`：表单模式走 SchemaForm 必填校验 | `130-143` |

`schema` 为空对象或缺省时，插件只能编辑原始 JSON。Monaco 的 schema 诊断通过 `updateAllSchema()` 把 `http:///api/plugin/schema/{code}` 注册为 JSON schema（`hooks/index.ts:8-22`），依赖 `/api` 代理；小程序无 Monaco，必须改为原生表单/文本 JSON。

#### 2.4.6 资源插件绑定 `PluginListForm.vue`

| 操作 | 行为 | 证据 |
| --- | --- | --- |
| 展示已绑定 | 每个绑定一个彩色 `el-tag`，标签文本：原生用 code，Wasm 用基础插件展示名 | `166-190`、`wasmPlugin.ts:167-169` |
| 删除绑定 | 点击 tag 关闭图标立即从本地数组移除，**无确认**；保存资源后才持久化 | `168-175` |
| 拖拽排序 | 拖拽图标，`dragstart/drop` 重排数组；提示“仅当优先级相同时，拖拽顺序决定执行顺序” | `115-126`、`177-188` |
| 打开绑定详情 | 点击 tag 主体 | `175`、`128-134` |
| 添加插件 | 打开 640px 抽屉；插件选择 + 优先级 | `191-193`、`196-233` |
| 优先级 | 整数 `-2147483648..2147483647`，默认 0，step 1，precision 0 | `47-48`、`214-226` |
| 保存绑定 | 先调用 `PluginSelect.save()`（可能创建自定义实例/Wasm 绑定），再 push 当前绑定；路由范围会阻止同 code 重复 | `97-113` |
| 重复限制 | 仅 `bindingScope === 'route'` 检查 `hasPluginCodeBinding`；rule/backend/gateway 不限制 | `99-109`、`utils/wasmPlugin.ts:182-185` |
| 实例配置刷新 | 组件挂载时 `GET /config/plugin-all`；失败保留 `configError` | `56-67`、`161-163` |
| 绑定详情保存 | 若改了展示名称/运行时配置，`PUT /config/plugin`；随后更新本地 `binding.priority` | `140-159` |

绑定 tag 的 key 为 `${keyPluginId}-${index}`，允许同一实例多次绑定（路由范围除外）。

后端插件的数组顺序也有运行时语义：SDK 注释明确“第一个 plugin 是最外层”（`sdk/admin-client/dist/model/SgBackendRef.d.ts` 末段）。因此拖拽不仅影响同优先级顺序，也影响后端插件链的包裹顺序；迁移时不能把绑定集合当无序集合。

资源侧传入的 `bindingName` 约定（用于生成 Wasm 绑定实例的稳定名称/展示名，`PluginSelect.vue:323-352`）：

| 作用域 | `binding-scope` | `binding-name` | 证据 |
| --- | --- | --- | --- |
| 路由级 | `route` | `${route_name}-route` | `RouteEditorDrawer.vue:1097-1101` |
| 规则级 | `rule` | `${route_name}-rule-${ruleIndex+1}` | `RouteEditorDrawer.vue:1104-1109` |
| HTTP 后端 | `backend` | `${route_name}-rule-${ruleIndex+1}-backend-${backendIndex+1}` | `RouteEditorDrawer.vue:1076-1080` |
| MCP 后端 | `backend` | `${route_name}-mcp-backend-${backendIndex+1}` | `RouteEditorDrawer.vue:996-1000` |
| 网关级 | `gateway` | `${gateway.name}-gateway` | `GatewayForm.vue:264-268` |

Wasm 绑定实例名最终为 `bind-<fnv1a32(scope:owner:plugin)>`，其中 owner 优先取 `bindingName`；展示名为 `scope / owner / plugin`（`PluginSelect.vue:323-352`、`wasmPlugin.ts:223-230`）。迁移时若改变这些字符串，会导致同一资源重新绑定后生成新的实例名，旧绑定成为孤儿。

#### 2.4.7 绑定详情 `PluginBindingDetail.vue`

| 区域 | 内容 | 证据 |
| --- | --- | --- |
| 告警 | 加载失败显示错误；实例不存在显示“挂载引用的插件实例不存在，挂载关系已保留” | `79-93`、`20-23` |
| 类型 | 原生/Wasm（按 code 前缀判断） | `34-36`、`96-98` |
| 插件名 | 原生 code；Wasm 基础插件展示名 | `38`、`99-101` |
| 实例名称 | 可编辑 `displayName` 输入框，并显示“当前实例：...” | `102-105` |
| 优先级 | 整数输入 + 范围提示 | `106-118` |
| Wasm 摘要 | 镜像 URL（`spec.image_url ?? spec.url`）、`plugin_name`、`fail_strategy` | `121-132` |
| 运行时配置 | `PluginRuntimeConfigEditor` | `133-137` |
| 保存 | 组装 `{priority, config}` 抛出；父组件负责 PUT 实例并更新绑定 | `55-67`、`PluginListForm.vue:141-159` |

重要副作用：展示名称编辑的是**共享插件实例**的 `display_name`，不是绑定副本；同一实例被多个资源引用时，改名会影响所有引用处（`PluginBindingDetail.vue:46-65`、`PluginListForm.vue:145-153`）。

#### 2.4.8 插件选择 `PluginSelect.vue`

- 分类：原生 / Wasm 扩展（`50`、`619-627`）。
- 原生：
  - 插件类型下拉（过滤 Wasm code 与 `blockedPluginCodes`，按展示名排序）。
  - 配置方式：引用已有 named 配置 / 新建原生配置。
  - 引用模式只列 named 实例，用 `keyPluginId` 字符串作为选中值。
  - 新建模式显示 `PluginForm`，保存时 `POST /config/plugin`，`kind` 固定 `named`，名称随机 `<code>-<uid>`。
  - 证据：`222-231`、`257-280`、`461-483`、`660-700`。
- Wasm：
  - 只列 `kind=named`、`spec.binding_scope === undefined` 的基础配置；按展示名排序。
  - 绑定配置方式：默认配置 / 镜像 Schema 表单 / YAML（代码还支持 xml，但 UI 不提供）。
  - Schema 通过已保存实例 ID 调 `GET /plugin/wasm/schema`，失败显示错误，空 schema 显示提示。
  - 绑定名称/展示名按 `bindingScope` + `bindingName` + 基础插件名生成稳定 ID：`bind-<fnv1a32(scope:owner:plugin)>`；展示名 `scope / owner / plugin`。
  - 保存时：
    - 已有绑定：`PUT /config/plugin`。
    - 新绑定：`POST /config/plugin`。
    - 写入的 spec 会复制基础配置的全部字段（含 `image_url`、`oci_auth`、`http_headers`），再覆盖 `binding_config_mode/scope/base_plugin/owner/display_name` 与 `plugin_config/default_config`。
  - 证据：`233-254`、`323-352`、`392-409`、`485-516`、`wasmPlugin.ts:351-394`。
- 初始值：挂载时并行拉 `attr-all` 与 `plugin-all`；无初始 code 时默认选中第一个原生插件；已有绑定会解析 `binding_base_plugin` 并回填（`579-612`、`536-577`）。

#### 2.4.9 运行时配置编辑器 `PluginRuntimeConfigEditor.vue`

| 项 | 内容 | 证据 |
| --- | --- | --- |
| 支持模式 | 原生：`schema`/`json`；Wasm：`schema`/`json`/`yaml`/`xml` | `21`、`37-39` |
| 初始模式 | Wasm 且已存 `binding_config_mode` 为 yaml/xml 时用该模式，否则 `schema`；无 schema 自动退 JSON | `52-64`、`86-92` |
| Schema 加载 | 原生走 `/plugin/schema/{code}`；Wasm 找 `binding_base_plugin` 对应基础配置，走 `/plugin/wasm/schema` | `66-94` |
| 模式切换 | JSON→其他先解析；XML→Schema 禁止；YAML 用 `yaml` 库解析 | `105-140` |
| 保存结果 | 原生：整体替换 `spec`；Wasm：合并 `binding_config_mode`、`plugin_config`、`default_config`，保留镜像/绑定元数据 | `142-158`、`utils/pluginRuntimeConfig.ts:16-48` |
| 错误 | 非法 JSON/YAML 弹 `ElMessage.error` 并阻止保存 | `107-115`、`145-151` |

#### 2.4.10 Schema 表单（`SchemaForm` / `SchemaArrayField` / `SchemaJsonField`）

这是迁移最容易“看起来能用、实际降级”的部分。

| 能力 | 实现 | 证据 |
| --- | --- | --- |
| `$ref` | 仅支持本地 `#/...`；解析后合并外层 title/description | `utils/schemaEditor.ts:15-43` |
| `allOf` | 合并所有分支 | `44-50` |
| `anyOf` | 取第一个非 `null` 分支（Option<T> 语义） | `52-58` |
| `oneOf` | 仅当所有分支同标量类型且都有 enum 时，扁平化为一个 enum | `60-67` |
| 嵌套对象 | 有 `properties` 且深度 ≤3 时递归表单；超过深度用 JSON textarea | `76-82`、`schemaEditor.ts:2`、`77-82` |
| 数组 | 对象项递归；对象/数组/`additionalProperties`/无类型项用每项 JSON 编辑；布尔/数字/字符串用对应控件 | `SchemaArrayField.vue:35-80`、`84-114` |
| 默认值 | `initDefaults` 只为缺失字段填 `default`；对象递归；不给数字填 0（保持 undefined） | `SchemaForm.vue:103-116` |
| 必填 | 仅顶层 `required` 的 undefined/null/空字符串；**不递归校验嵌套对象/数组** | `118-129` |
| 枚举 | **没有 enum 下拉**；字段最终落到 `el-input` 文本输入（数组项同理） | `SchemaForm.vue:166-190`、`SchemaArrayField.vue:95-110` |
| 数字 | `el-input-number` 固定 `min=0`，无法输入负数 | `SchemaForm.vue:169-171` |
| 组合变体 | 非标量 `oneOf` 不展开，通常因缺少 `type` 被当作任意 JSON，走 JSON textarea | `schemaEditor.ts:60-67`、`90-103` |
| i18n 标题 | 支持 `x-title-i18n.zh-CN/en`、`x-description-i18n`；否则 title 或 key 转 Title Case | `44-60` |
| JSON 字段 | 只有合法 JSON 才回写模型；错误保留文本并显示红色错误 | `SchemaJsonField.vue:10-43` |

因此，设计文档中“嵌套 schema 编辑”在旧实现里的真实边界是：**最多三层对象递归、数组对象可递归、标量 oneOf 可合并 enum 但不渲染下拉、复杂 oneOf 回退 JSON、无嵌套必填校验、数字不允许负数**。迁移不能只实现一个 JSON 文本框，也不能声称已覆盖 enum/oneOf 选择。

#### 2.4.11 自定义 Wasm 插件抽屉 `ThirdPartyWasmDrawer.vue`

抽屉 76%，五个页签：插件定义、默认配置、匹配规则、高级参数、OCI 认证、HTTP(S) 认证（`301-554`）。

#### 插件定义

| 字段 | 默认 | 校验/说明 | 证据 |
| --- | --- | --- | --- |
| 插件名称 `instance_name` | `custom-wasm-plugin` | 保存时规范化为小写字母/数字/中划线；创建后锁定；生成 `wasm.{name}.json` | `47`、`99-114`、`204-212`、`234-236`、`330-333` |
| 显示名称 `display_name` | 空 | 空则跟随插件名称；写入顶层 `display_name` 与 `spec.display_name` | `48`、`99-104`、`437`、`464` |
| 描述 `description` | 空 | 自由文本 | `49`、`338-340` |
| 镜像地址 `image_url` | 空（必填） | 支持 `oci://registry/repo:tag`、`registry/repo:tag`、`http(s)://`、`file://` 或本地路径 | `50`、`81-86`、`225-232`、`341-344` |
| Schema 路径 `schema_path` | `schema.json` | 供绑定表单读取 | `58`、`345-348` |
| 生效阶段 `phase` | `UNSPECIFIED_PHASE` | `UNSPECIFIED_PHASE/AUTHN/AUTHZ/STATS` | `33-38`、`359-368` |
| 优先级 `priority` | 0 | `-100000..100000`（注意与资源绑定 int32 不同） | `51`、`369-371` |
| 拉取策略 `image_pull_policy` | `IfNotPresent` | `UNSPECIFIED_POLICY/IfNotPresent/Always`；选 `Always` 会自动把 `use_cache` 置 false | `40-44`、`52`、`116-118`、`372-381` |
| 拉取凭证 `image_pull_secret` | 空 | 字符串 | `53`、`383-386` |
| SHA-256 `sha256` | 空 | 自由文本，`sha256:<hex>` | `57`、`387-390` |
| 插件名 `plugin_name` | 跟随实例名 | 运行时逻辑名 | `54`、`105-107`、`391-394` |
| 失败策略 `fail_strategy` | `fail_open` | `fail_open/fail_close` | `55`、`395-400` |

#### 默认配置

| 字段 | 默认 | 说明 | 证据 |
| --- | --- | --- | --- |
| `default_config_disable` | false | 禁用后运行时配置为 `{}`，但仍保存 `default_config` | `59`、`415-417`、`283-289` |
| `default_config_text` | `{}` | 必须是合法 JSON；禁用时 textarea 禁用 | `60`、`418-426`、`245-247` |

#### 匹配规则

| 字段 | 默认 | 说明 | 证据 |
| --- | --- | --- | --- |
| `match_rules_text` | `[]` | 必须是 JSON array；保存后写入 `spec.match_rules`，非空时再写入 `plugin_config._rules_` | `61`、`431-449`、`247`、`283-289` |

#### 高级参数

| 字段 | 默认 | 范围/说明 | 证据 |
| --- | --- | --- | --- |
| `plugin_root_id` | `<instance>-root` | 自由文本 | `62`、`108-110`、`461-464` |
| `plugin_vm_id` | `<instance>-vm` | 自由文本 | `63`、`111-113`、`465-467` |
| `module_cache_key` | 空 | 自由文本 | `64`、`469-472` |
| `use_cache` | true | 布尔 | `65`、`473-475` |
| `vm_pool_size` | 1 | `1..64` | `66`、`476-479` |
| `wait_vm_pool_size` | 0 | `0..64` | `67`、`480-482` |
| `clusters_text` | `{}` | JSON object | `74`、`483-485`、`274` |
| `limits_text` | `{}` | JSON object | `75`、`486-488`、`275` |

#### OCI 认证

`registry`、`username`、`password`、`bearer_token`、`identity_token`；后三者是 password 输入框；全空时保存会删除 `spec.oci_auth`（`68-72`、`192-200`、`493-521`、`444-449`）。

#### HTTP(S) 认证

- 键值行数组；名称和值必须同时填写，否则保存报错“HTTP 请求头的名称和值必须同时填写”。
- 值使用 password 输入框掩码显示。
- 保存为 `spec.http_headers`；全空时删除该字段。
- 证据：`12-16`、`73`、`202-223`、`523-546`、`451-456`。

#### 保存

- 新建 `POST /config/plugin`，编辑 `PUT /config/plugin`。
- 成功提示“请执行全局重载后生效”，`emit('saved', config)` 并关闭。
- 编辑已有基础插件后，`PluginPanel.onThirdPartyWasmSaved` 会找出所有 `spec.binding_base_plugin === 基础插件 name` 的绑定，逐个 `PUT /config/plugin`，把模块来源同步到新定义（`PluginPanel.vue:593-610`、`wasmPlugin.ts:309-326`）。
- 证据：`ThirdPartyWasmDrawer.vue:279-297`。

#### 已知问题

- 表单标签“镜像地址L”有拼写错误（`341`）。
- 新建时 `instance_name` 变化会联动 `display_name`/`plugin_name`/root/vm，但只要用户改过其中一个，就不再联动（`99-114`）；行为较隐晦。
- HTTP 头值和 OCI 密码会以明文写入插件配置；UI 只做显示掩码，不是加密（`535-540`）。
- 绑定配置会复制基础插件的 `oci_auth` 与 `http_headers`（`wasmPlugin.ts:351-394` 的 `...baseSpec`），敏感信息在多个绑定配置中重复。

#### 2.4.12 插件中心状态与边界

- `pluginAttrs` 加载失败时会二次降级；如果降级也失败，`finally` 仍会把 `pluginsLoading` 置 false，但 `onMounted` 的 Promise 会以未处理拒绝结束，页面表现为“空列表 + 无提示”（`333-351`）。
- Wasm 实例加载失败静默为空（`308-317`）。
- 当前网关加载失败静默为 undefined，全局开关随后提示“当前网关配置加载失败”（`319-331`、`384-391`）。
- 原生插件实例删除成功没有成功提示（`522-539`）。
- 创建/更新插件成功后 `emit('changed')`，父页面会重建整个面板（`569-582`、`Plugins.vue:48`）。
- 插件属性缓存是服务端 1 小时（`spacegate/binary/admin-server/src/service/plugin.rs:26`、`47-74`）；页面“刷新配置”只重建前端，不会强制刷新服务端缓存，也不会调用 `/plugin/refresh`。
- 原生插件详情只展示 named 实例；anon/mono 的实例管理能力缺失（`791-842`、`matchesInstanceSearch` 440-445）。
- 插件 `display_name` 是顶层字段，不进入实例 ID；删改名称不影响绑定（`pluginInstance.ts:28-56`、`admin-server/src/service/config.rs:22-40`）。

---

### 2.5 AI 排队限流 `/ai-queue`

#### 2.5.1 路由与入口

- 路由 `/ai-queue`（`src/router.ts:11`），侧栏“AI 限流”（`src/App.vue:36`）。
- 页面自身包含两块：插件宿主配置（抽屉）+ 租户队列配额（内嵌表格）。

#### 2.5.2 页面级操作

| 操作 | 行为 | API | 证据 |
| --- | --- | --- | --- |
| 加载队列实例 | 进入页面拉 `GET /config/plugins/wasm`，找 `spec.plugin_name === 'ai-gateway-queue'` 或 `name === 'ai-gateway-queue'` | `getConfigPluginsByCode('wasm')` | `src/views/AiQueueRateLimit.vue:47-57` |
| 配置插件宿主 | 打开 `AiGatewayQueueDrawer`，传入找到的实例（可为空） | — | `81-83`、`97-101` |
| 保存宿主后 | `onSaved` 重新拉队列实例 | 同上 | `59-61` |
| 租户配额 | 直接渲染 `TenantRateLimitTable`（页面内一份） | 见 2.5.4 | `94` |

#### 2.5.3 宿主配置抽屉 `AiGatewayQueueDrawer.vue`

| 页签/字段 | 内容 | 默认/校验 | 证据 |
| --- | --- | --- | --- |
| 基础配置 · Wasm URL `url` | 插件制品地址，支持 `file://`、`http(s)://`、`oci://` | 空 | `103-110`、`269-274` |
| 基础配置 · 插件名称 `plugin_name` | 运行时插件名 | `ai-gateway-queue` | `105`、`275-280` |
| 基础配置 · 失败策略 `fail_strategy` | `fail_close`/`fail_open` | `fail_close` | `106`、`281-289` |
| 基础配置 · 队列后端地址 `service_url` | 写入 `clusters['ai-gateway-service']` | `http://127.0.0.1:18080` | `107`、`162`、`290-295` |
| 基础配置 · 普通 VM 池 `vm_pool_size` | 处理短请求的 VM 数 | 4，最小 1 | `108`、`296-301` |
| 基础配置 · Wait VM 池 `wait_vm_pool_size` | 长等待请求专用，0 表示不用 wait | 4，最小 0 | `109`、`302-307` |
| 基础配置 · 插件内部参数 | 由 AI Gateway schema 驱动的 `SchemaForm`，写入 `plugin_config` | schema 缺失时显示警告，不阻止保存宿主字段 | `310-318`、`32-36` |
| 队列配额 | 再渲染一份 `TenantRateLimitTable` | — | `320-322` |
| 文档 | `GET /v1/admin/plugins/ai-gateway-queue/readme`，用 `<pre>` 原样显示（不是 Markdown 渲染） | 失败显示错误 | `323-333`、`PluginMarkdown.vue:1-23` |
| 队列观测 | 空态“V1 先预留观测页” | — | `334-336` |

数据加载与保存：

- 同时请求 schema 与 readme，使用 `Promise.allSettled`；任一失败分别记录错误，仍打开抽屉（`125-150`）。
- 读取已有实例：`spec.url`、`plugin_name`、`fail_strategy`、`clusters['ai-gateway-service']`、VM 池、`plugin_config`；`plugin_config` 缺失时用 schema 的 `x-example-raw` 解析（`157-178`）。
- 保存：在现有 spec 上覆盖上述字段，`clusters` 合并保留其他键；无实例时创建 `code=wasm, kind=named, name=ai-gateway-queue, display_name='AI 网关排队限流'`（`184-212`）。
- 保存调用 admin-server `POST/PUT /config/plugin`；成功提示，`emit('saved')`，关闭；`finally` 只清 saving，**不捕获错误**（`214-229`）。
- 错误提示“AI Gateway 后端不可用，请检查 ai-gateway-service :18080”只在 schema 失败时显示（`250-257`）；readme 失败单独提示（`324-331`）。

数据来源与安全注意：

- schema/readme 走 `api/aiGateway.ts` 的 `fetch`，base URL 为 `VITE_AI_GATEWAY_BASE_URL` 或 `/ai-gateway`（`40-58`、`79-109`）。
- 这条链路**不经过 SDK**，不带 `X-Client-Version`，也不处理 401/409；只把非 2xx 的响应文本抛成 Error（`60-77`）。
- 同源部署时浏览器默认会带上 `jwt` Cookie 到 `/ai-gateway`，与 admin-server 的 Cookie 同名；是否应该隔离需在新架构中明确。

#### 2.5.4 租户队列配额 `TenantRateLimitTable.vue`

#### 操作清单

| 操作 | 行为/确认 | API | 证据 |
| --- | --- | --- | --- |
| 查询 | 按 tenant/model/path/policy 过滤；空字符串不发送 | `GET /v1/admin/tenant-rate-limits?...` | `232-243`、`373-384`、`api/aiGateway.ts:111-120` |
| 新增 | 清空右侧编辑器，进入 create 模式 | — | `250-254`、`383`、`438` |
| 编辑 | 用行数据填充编辑器，记录 `key` | — | `256-260`、`428` |
| 保存 | 前端校验后 `PUT /v1/admin/tenant-rate-limits`（body 为规则）；成功后转为 edit 模式、记录返回 key、刷新列表 | `upsertTenantRateLimit` | `266-297`、`api/aiGateway.ts:132-137` |
| 删除 | 确认“确认删除队列配额 {key}？”；成功后若删除的是当前编辑项则重置为新增；刷新 | `DELETE /v1/admin/tenant-rate-limits`（**JSON body**） | `325-345`、`api/aiGateway.ts:139-144` |
| 解析当前请求 | 用 tenant/model/path/policy 调 resolve，显示命中/回退、生效配额、候选 key | `GET /v1/admin/tenant-rate-limits/resolve?...` | `299-323`、`484-507` |
| 取消 | 调用 `openCreate()`，即重置为空白新增，不是恢复编辑前值 | — | `511`、`250-254` |
| 重试 | 接口错误告警内“重试”重新查询 | — | `360-371` |

#### 字段与校验

| 字段 | 类型 | 必填/默认 | 校验/单位 | 证据 |
| --- | --- | --- | --- | --- |
| `tenant` | string | 必填，默认空 | 非空；不能含 `:`；最大长度由后端/contract 约束 | `266-274`、`262-264` |
| `model` | string | 可选 | 不能含 `:`；空表示全部模型 | `445-448` |
| `path` | string | 可选 | 不能含 `:`；空表示全部路径 | `449-452` |
| `policy` | `abandon`\|`queue`\|`wait`\|`''` | 可选，默认空 | 空表示全部模式 | `453-460` |
| `rps` | number | 必填，默认 10 | >0；单位“每秒入队上限” | `31-39`、`275-278`、`462-465` |
| `burst` | number | 必填，默认 20 | >0；令牌桶容量 | `466-469` |
| `cost` | number | 必填，默认 1 | >0 且 ≤burst；单请求令牌数 | `470-473`、`279-282` |
| `ttl_secs` | number\|undefined | 可选 | >0 或留空表示永久；单位秒 | `283-286`、`475-478` |

#### Redis key 预览

- 格式：`ai:tenant:ratelimit:<tenant>[:model:<model>][:path:<path>][:policy:<policy>]`。
- tenant 为空显示占位 `ai:tenant:ratelimit:{tenant}`。
- 非法字符（除 `a-zA-Z0-9:_-.`）替换为 `_`。
- 证据：`172-183`。

#### 表格列

租户、匹配范围（租户/模型/路径/模式/组合）、模型（空显示“全部模型”）、路径（空显示“全部路径”）、队列模式（空显示“全部模式”）、RPS、Burst、Cost、TTL（空显示“永久”）、剩余 TTL（无显示 `-`）、队列配额键、操作（编辑/删除）。证据：`388-432`、`204-215`。

#### 状态与边界

- 接口失败：清空列表，显示“队列配额接口不可用 + 错误文本”，带重试（`232-243`、`360-371`）。
- 表格固定高度 520，无分页；全量渲染（`388`）。
- 保存/删除失败显示后端文本（`294-296`、`342-344`）。
- `el-input-number` 的 `clearable` 属性并不被 Element Plus 支持（`476`）；TTL 一旦填过，UI 可能无法清空为“永久”，需要实测，迁移时应提供显式“永久/有限”开关。
- 策略下拉的 label 在英文界面仍为中文（`378-381`、`455-458`）。
- `AiGatewayQueueDrawer` 的“队列配额”页签和页面主体各挂载一个 `TenantRateLimitTable`，两者状态独立、各自请求（`AiQueueRateLimit.vue:94`、`AiGatewayQueueDrawer.vue:321`）。

#### 2.5.5 跨页依赖

- 宿主配置保存的是 `code=wasm` 的 named 实例，插件中心会把它从 Wasm 卡片中排除（`wasmPlugin.ts:101`），但 AI 队列页面会按名称找到它。
- 保存宿主配置后没有自动重载；页面文案要求“保存后执行全局重载”（`AiQueueRateLimit.vue:25`）。
- 租户配额存在 AI Gateway 的 Redis 中，与 admin-server 配置版本无关，不走 409 语义。

---

### 2.6 实例运维 `/instances`

#### 2.6.1 路由与入口

- 路由 `/instances`（`src/router.ts:12`），侧栏“实例”（`src/App.vue:37`），顶栏徽标/图标也进入（`137-139`、`153`）。
- 页面不读取 `gatewayName` query，自己维护重载目标网关注释。

#### 2.6.2 操作清单

| 操作 | 行为/确认 | API | 证据 |
| --- | --- | --- | --- |
| 加载 | 依次拉网关名、实例 ID 列表、健康表；默认选中第一个网关 | `getConfigNames`、`discoveryInstanceList`、`discoveryInstanceHealth` | `src/views/Instances.vue:70-85` |
| 刷新 | 重新执行加载；无 loading 掩码在按钮上，但表格有 `v-loading` | 同上 | `165`、`170` |
| 选择重载目标网关 | 下拉，仅影响“网关重载” | — | `159-164` |
| 网关重载 | 每行按钮；未选网关时警告；无二次确认；按行显示 loading | `POST /discovery/instance/reload/gateway?instance&gateway` | `113-132`、`192-200` |
| 全局重载 | 每行危险色按钮；**无二次确认**；按行 loading | `POST /discovery/instance/reload/global?instance` | `99-111`、`201-208` |
| 复制实例 ID | 写剪贴板并提示；失败静默 | — | `134-141`、`171-177` |
| 空态 | “启动 spacegate 网关后，实例将自动注册” | — | `212-214` |

#### 2.6.3 列

| 列 | 内容 | 证据 |
| --- | --- | --- |
| 实例 ID | `<code>` + 复制按钮 | `171-177` |
| 控制 API | 固定显示 `http://{{ id }}`（假设实例 ID 就是 host:port） | `179-183` |
| 健康状态 | true 在线/ success；false 离线/danger；undefined 未知/info | `87-97`、`184-188` |
| 操作 | 网关重载（warning）/ 全局重载（danger） | `189-211` |

#### 2.6.4 状态与边界

- 加载失败：清空网关与实例并提示（`77-82`）。
- 健康表缺失某实例时 `healthy` 为 undefined，显示“未知”（`75-78`、`93-97`）。
- 重载请求使用 `Api.getClient().axiosInstance.post` 原始调用（`102-104`、`120-125`），仍会带 SDK 的 `X-Client-Version`，因为共用同一个 axios 实例。
- 重载后不刷新列表、不重新拉健康；只弹成功/失败（`99-132`）。
- 同一行两个按钮各自 loading；其他行按钮仍可点击，避免重复点击只对同一行同一动作生效（`100`、`118`、`196`、`204`）。
- 没有路由级重载入口；SDK 与后端都有 `reload/route`，但 UI 未使用（`dist/api/index.js:289-292`、`admin-server/src/service/discovery.rs:193-213`）。
- 实例 ID 复制失败静默（`134-141`）。

#### 2.6.5 跨页依赖

- 所有配置保存后都要到这里重载才生效；页面本身不感知配置变化。
- 顶栏每 5 秒的健康轮询与本页的健康加载互相独立。
- 重载 POST 会推进服务端版本（见 1.5），可能让其他标签页下一次写操作 409。

---

### 2.7 登录（非独立路由）

#### 2.7.1 入口

- 任意 `/api` 请求返回 401 时，由 `src/main.ts` 的响应拦截器弹出 `Login` 组件对话框（不可关闭、不可点遮罩、不可 ESC）。
- 没有 `/login` 路由，也没有主动退出登录按钮。

#### 2.7.2 字段与行为

| 字段 | 类型 | 说明 | 证据 |
| --- | --- | --- | --- |
| 用户名 | string | 实际传给后端 `ak` | `Login.vue:6`、`8-10` |
| 密码 | string | `show-password`，传给后端 `sk` | `Login.vue:7`、`26-29` |
| 登录 | 按钮（对话框确认） | `POST /auth/login {ak, sk}`；成功后 `location.reload()`；失败被 `Login.login` 静默吞掉 | `Login.vue:8-14`、`main.ts:54-57` |

#### 2.7.3 后端行为

- `sk` 与启动参数 `--sk` 的 SHA-256 比较；不匹配返回 401。
- 配置了 `--key` 时签发 HS256 JWT，`sub=admin`、`username=ak`、`exp=+3600s`，写入 HttpOnly Cookie。
- 未配置 `--key` 时，即使配置了 `--sk`，登录成功也不发 JWT；后续请求也不校验 JWT（开发开放模式）。
- 证据：`spacegate/binary/admin-server/src/service/auth.rs:21-51`、`src/mw/authentication.rs:25-43`。

#### 2.7.4 状态与边界

- 登录失败没有错误提示；对话框确认回调仍然执行 `location.reload()`，会再次 401，形成“输入错误→刷新→再弹框”的循环（`main.ts:54-57`、`Login.vue:8-14`）。
- `Login` 组件的表单标签 `type="password"` 放在 `el-form-item` 上（`Login.vue:26`），不是输入框属性，属于写法瑕疵。
- 401 只覆盖 SDK/axios 链路；AI Gateway 的 `fetch` 401 不会触发登录框。

---

### 2.8 未路由/历史代码（迁移时不要误当现状）

| 文件/组件 | 状态 | 说明 |
| --- | --- | --- |
| `src/components/Menu.vue`、`Header.vue` | 未使用 | `App.vue` 自己实现侧栏和顶栏 |
| `src/views/index.ts` | 未使用 | 只导出包装组件 |
| `src/views/Gateway.vue`、`Route.vue`、`Instance.vue`、`Plugin.vue` | 未路由 | 包装子模块旧页面，`router.ts` 未引用 |
| `components/config/src/views/Gateway.vue` | 未使用 | 单网关编辑 + 保存，未接入活动路由 |
| `components/config/src/views/Routes.vue` | 未使用 | 旧的卡片式路由页，含“创建重名覆盖确认”；活动页是 `src/views/Routes.vue` |
| `components/config/src/components/SelectGateway.vue` | 未使用 | 旧网关选择器，含新增/删除/导入导出；活动页用 `ConsoleGatewaySelect.vue` |
| `RouteForm.vue`、`McpRouteForm.vue`、`RouteRuleForm.vue`、`RouterMatchForm.vue`、`BackendForm.vue` | 未使用（活动路由抽屉自绘） | **后端发现选择器只存在于这里**；活动 UI 没有 `/discovery/backends` 入口 |
| `components/config/src/components/InstanceList.vue` | 未使用 | 旧实例页；全局重载只弹提示不调 API |
| `ConfigPanel.vue` | 未使用 | 纯布局插槽 |
| `hooks/useGateway.ts` | 仅被死代码 `views/Gateway.vue` 使用 | 封装单网关读取/保存；活动页自行实现 |
| `service/index.ts` 的 `SpacegateService` | 未安装 | 只是 `Api.setClient` 的插件包装；`main.ts` 直接调用 `Api.setClient` |
| `hooks/updatePluginCodes` | 被 `PluginForm` 调用但结果未使用 | 仅额外拉一次 `/plugin/list` |
| `components/config/src/index.ts` | 组件注册入口 | 注册组件后又用同一批 key 注册 views，views 实际上可能未注册；活动页直接 import 组件，不依赖全局注册 |

后端发现能力：`BackendForm` 挂载时调用 `GET /discovery/backends`，按 File/Host/K8sService 分组让用户选后端（`components/config/src/components/BackendForm.vue:32-67`、`70-118`）。因为活动路由抽屉未使用它，这是一个**在现网 UI 中已丢失的旧能力**。迁移时需决定是否恢复；设计文档要求保留既有交互，建议恢复。

---

## 3. 数据模型、枚举、默认值与单位汇总

### 3.1 网关

| 模型 | 关键字段 | 证据 |
| --- | --- | --- |
| `SgGateway` | `name`、`parameters`、`listeners[]`、`plugins[]` | `sdk/admin-client/dist/model/SgGateway.d.ts` |
| `SgParameters` | redis_url、4 个 pool 参数、log_level、lang、ignore_tls_verification、enable_x_request_id、observability | `dist/model/SgParameters.d.ts` |
| `SgListener` | name、ip、port、protocol、hostname | `dist/model/SgListener.d.ts` |
| `SgProtocolConfig` | `{type:'http'}` 或 `{type:'https', tls}` | `dist/model/SgProtocolConfig.d.ts` |
| `SgTlsConfig` | mode、key、cert、http2 | `dist/model/SgTlsConfig.d.ts` |
| `ObservabilityConfig` | enabled、service_name、otlp_endpoint、protocol(grpc/http)、traces、metrics、logs | `dist/model/ObservabilityConfig.d.ts` |

### 3.2 路由

| 模型 | 关键字段 | 证据 |
| --- | --- | --- |
| `SgRoute` | `SgHttpRoute` 或 `SgMcpRoute` 联合 | `dist/model/SgRoute.d.ts` |
| `SgHttpRoute` | route_name、hostnames、plugins、rules、priority | `dist/model/SgHttpRoute.d.ts` |
| `SgHttpRouteRule` | matches、plugins、backends、timeout_ms、timeout_mode、balance_policy | `dist/model/SgHttpRouteRule.d.ts` |
| `SgHttpRouteMatch` | path、header、query、method | `dist/model/SgHttpRouteMatch.d.ts` |
| `SgHttpPathMatch` | kind Exact/Prefix/RegExp、value、replace | `dist/model/SgHttpPathMatch.d.ts` |
| `SgHttpHeaderMatch` | exact: name/value/replace；reg_exp: name/re/replace | `dist/model/SgHttpHeaderMatch.d.ts` |
| `SgHttpQueryMatch` | exact: value{key,value}；regular: value{key,re} | `dist/model/SgHttpQueryMatch.d.ts` |
| `SgBackendRef` | host、port、timeout_ms、timeout_mode、protocol、downgrade_http2、weight、plugins | `dist/model/SgBackendRef.d.ts` |
| `BackendHost` | Host{host} / K8sService{name,namespace} / File{path} | `dist/model/BackendHost.d.ts` |
| `SgMcpRoute` | kind='MCPRoute'、transport、path、legacy_sse、backends、plugins、timeout_mode、session_affinity | `dist/model/SgMcpRoute.d.ts` |
| `SgMcpTransport` | `streamable_http`\|`legacy_sse` | `dist/model/SgMcpTransport.d.ts` |
| `McpSessionAffinity` | `mcp_session`\|`none` | `dist/model/McpSessionAffinity.d.ts` |
| `TimeoutMode` | `request`\|`disabled` | `dist/model/TimeoutMode.d.ts` |
| `SgBalancePolicy` | `random`\|`ip_hash`\|`mcp_session` | `dist/model/SgBalancePolicy.d.ts` |

### 3.3 插件

| 模型 | 关键字段 | 证据 |
| --- | --- | --- |
| `PluginBinding` | priority（int）、code + kind/name 或 uid | `dist/model/PluginBinding.d.ts` |
| `PluginInstanceId` | code + kind/name/uid | `dist/model/PluginInstanceId.d.ts` |
| `PluginConfig` | display_name、spec（任意 JSON）、code + kind/name/uid | `dist/model/PluginConfig.d.ts` |
| `PluginAttributes` | meta、mono、code | `dist/model/PluginAttributes.d.ts` |
| `PluginMetaData` | title、authors、description、version、homepage、repository | `dist/model/PluginMetaData.d.ts` |

插件实例 ID 的字符串形式（前端 key）：`code-n-name`、`code-a-uid`、`code-m`（`utils/index.ts:222-231`）。

Wasm spec 约定字段（由前端构造，后端原样保存）：

| 字段 | 含义 | 证据 |
| --- | --- | --- |
| `version` | 固定 0 | `utils/wasmPlugin.ts:411` |
| `category` | `custom` | `412` |
| `built_in` | false | `413` |
| `title` | 显示名或实例名 | `414` |
| `url` / `image_url` | 模块来源 | `415-416` |
| `image_repository` / `image_version` | 由 image_url 解析 | `417-418`、`241-261` |
| `phase` / `priority` / `image_pull_policy` / `image_pull_secret` | 调度与拉取 | `419-421`、`441` |
| `default_config_disable` / `default_config` / `match_rules` / `plugin_config` | 运行时配置；有 match rules 时 `plugin_config._rules_` | `422-425`、`283-289` |
| `plugin_name` / `plugin_root_id` / `plugin_vm_id` | 运行时标识 | `426-428` |
| `fail_strategy` / `use_cache` / `vm_pool_size` / `wait_vm_pool_size` | 运行策略 | `429-432` |
| `clusters` / `limits` | 对象 | `433-434` |
| `display_name` / `description` / `sha256` / `schema_path` / `module_cache_key` | 可选字符串，空值删除 | `437-442` |
| `oci_auth` | 可选对象，空对象删除 | `444-449` |
| `http_headers` | 可选对象，空对象删除 | `451-456` |
| `binding_config_mode` / `binding_scope` / `binding_base_plugin` / `binding_owner` / `binding_display_name` | 仅绑定配置 | `367-382` |

### 3.4 AI 租户限流

| 字段 | 类型 | 说明 | 证据 |
| --- | --- | --- | --- |
| tenant | string | 必填 | `api/aiGateway.ts:11-20` |
| model | string? | 可选 | 同上 |
| path | string? | 可选 | 同上 |
| policy | `abandon`\|`queue`\|`wait`\|`''` | 可选 | 同上 |
| rps | number | 每秒入队上限 | 同上 |
| burst | number | 突发容量 | 同上 |
| cost | number | 单请求消耗 | 同上 |
| ttl_secs | number? | 秒；空为永久 | 同上 |
| key | string | 列表返回的 Redis key | `api/aiGateway.ts:22-25` |
| ttl_remaining_secs | number? | 列表返回的剩余 TTL | 同上 |

解析结果包含：tenant/model/path/policy/rps/burst/cost、`matched_key?`、`fallback_global`、`candidate_keys[]`（`api/aiGateway.ts:27-38`）。

---

## 4. 后端接口清单与 contract 覆盖状态

状态图例：

- `P1-contract`：`spacegate-admin-contract/src/spacegate-contract.ts` 已有对应 procedure（P1 阶段），serv 上游适配器仍在实现中。
- `P1-needs-extension`：contract 有 procedure，但输入/过滤能力不足以支撑旧 UI 行为。
- `legacy-only`：只属于旧认证方式，新架构由 Hai IAM 取代。
- `unused`：旧 UI 活动页面未调用，仅在死代码或客户端库中存在。

### 4.1 Rust admin-server（base `/api`，开发代理去前缀）

| # | 方法/路径 | 旧 SDK 函数 | 使用位置 | 新 contract | 状态/备注 |
| --- | --- | --- | --- | --- | --- |
| 1 | `GET /config/names` | `getConfigNames` | 顶栏、概览、网关、路由、实例 | `config.names` | `P1-contract` |
| 2 | `GET /config/item/{name}` | `getConfigItem` | 死代码 `utils` | `config.get` | `unused` |
| 3 | `POST /config/item/{name}` / `PUT` | `postConfigItem`/`putConfigItem` | 未使用 | `config.update` | `unused` |
| 4 | `GET /config/item/{gw}/gateway` | `getConfigItemGateway` | 概览、网关、插件中心 | `gateway.get` | `P1-contract` |
| 5 | `POST /config/item/{gw}/gateway` | `postConfigItemGateway` | 新建网关 | `gateway.upsert(create=true)` | `P1-contract` |
| 6 | `PUT /config/item/{gw}/gateway` | `putConfigItemGateway` | 编辑网关、Wasm 全局启停 | `gateway.upsert(create=false)` | `P1-contract`；注意 config item id 与运行时 name 可能不同 |
| 7 | `DELETE /config/item/{gw}/gateway` | `deleteConfigItemGateway` | 网关删除 | `gateway.delete` | `P1-contract`；文件后端级联路由，K8s 后端不级联 |
| 8 | `GET /config/item/{gw}/route/names` | `getConfigItemRouteNames` | 概览、网关、路由 | `route.list`（返回完整项） | `P1-contract`；旧 UI 只取名称 |
| 9 | `GET /config/item/{gw}/route/all` | `getConfigItemAllRoutes` | 旧子模块路由页 | `route.list` | `unused`（活动页） |
| 10 | `GET /config/item/{gw}/route/item/{route}` | `getConfigItemRoute` | 概览、路由 | `route.get` | `P1-contract` |
| 11 | `POST /config/item/{gw}/route/item/{route}` | `postConfigItemRoute` | 新建/复制路由 | `route.upsert(create=true)` | `P1-contract` |
| 12 | `PUT /config/item/{gw}/route/item/{route}` | `putConfigItemRoute` | 编辑路由 | `route.upsert(create=false)` | `P1-contract` |
| 13 | `POST /config/item/{gw}/route/item/{route}/rename` | `renameConfigItemRoute` | 路由改名 | `route.rename` | `P1-contract` |
| 14 | `DELETE /config/item/{gw}/route/item/{route}` | `deleteConfigItemRoute` | 删除路由 | `route.delete` | `P1-contract` |
| 15 | `DELETE /config/item/{gw}/route/all` | `deleteConfigItemAllRoutes` | 未使用 | 无独立 procedure | `unused`；可由 gateway.delete 级联 |
| 16 | `GET /config/plugin-all` | `getConfigPluginAll` | 概览计数、插件选择器/绑定 | `plugin.list` | `P1-contract` |
| 17 | `GET /config/plugin?code&kind&name=`（anon 用 `uid`） | `getConfigPlugin` | 未直接使用 | `plugin.get` | `unused` |
| 18 | `GET /config/plugins/{code}` | `getConfigPluginsByCode` | 插件中心、插件选择、AI 队列 | `plugin.list` **无 code 过滤** | `P1-needs-extension`：需 `code` 查询参数或按 code 的 procedure；否则只能全量拉取后前端过滤，语义/性能都变 |
| 19 | `POST /config/plugin?code&kind&name&display_name` | `postConfigPlugin` | 原生/Wasm/AI 队列创建 | `plugin.upsert(create=true)` | `P1-contract` |
| 20 | `PUT /config/plugin?code&kind&name&display_name` | `putConfigPlugin` | 编辑实例/绑定/展示名 | `plugin.upsert(create=false)` | `P1-contract` |
| 21 | `DELETE /config/plugin?code&kind&name=`（anon 用 `uid`） | `deleteConfigPlugin` | 删除插件实例/定义 | `plugin.delete` | `P1-contract` |
| 22 | `GET /plugin/list` | `pluginList` | 插件中心降级、Monaco schema 注册 | `plugin.catalog` | `P1-contract` |
| 23 | `GET /plugin/attr-all` | `pluginAttrAll` | 插件中心、插件选择 | `plugin.attrAll` | `P1-contract` |
| 24 | `GET /plugin/attr/{code}` | `pluginAttr` | 插件中心降级、插件选择 | `plugin.attr` | `P1-contract` |
| 25 | `GET /plugin/schema/{code}` | `pluginSchema` | 插件表单、运行时编辑器 | `plugin.schema` | `P1-contract` |
| 26 | `GET /plugin/wasm/schema?query` | `getSavedWasmPluginImageSchema` | Wasm 绑定 schema、运行时编辑器 | `plugin.wasmSchema` | `P1-contract` |
| 27 | `POST /plugin/wasm/schema/preview` | `previewWasmPluginImageSchema` | **活动 UI 未使用** | `plugin.previewWasmSchema` | `P1-needs-extension`：contract 输入只有 `imageUrl/schemaPath/ociAuth`，缺 `httpHeaders`；后端支持 `http_headers`（`admin-server/src/service/plugin.rs:82-92`） |
| 28 | `POST /plugin/refresh` | 无 SDK 函数 | 未使用 | `plugin.refresh` | `unused`；但页面“刷新配置”很容易被误认为会触发它 |
| 29 | `POST /auth/login {ak,sk}` | `authLogin` | 401 弹窗 | 无（Hai IAM） | `legacy-only` |
| 30 | `GET /discovery/instance/list` | `discoveryInstanceList` | 概览、实例 | `discovery.instances` | `P1-contract` |
| 31 | `GET /discovery/instance/health` | `discoveryInstanceHealth` | 顶栏、概览、实例 | `discovery.health` | `P1-contract` |
| 32 | `POST /discovery/instance/reload/gateway?instance&gateway` | `discoveryInstanceReloadGateway` | 实例页 | `discovery.reloadGateway` | `P1-contract`；会推进版本 |
| 33 | `POST /discovery/instance/reload/global?instance` | `discoveryInstanceReloadGlobal` | 实例页 | `discovery.reloadGlobal` | `P1-contract`；会推进版本 |
| 34 | `POST /discovery/instance/reload/route?instance&gateway&route` | `discoveryInstanceReloadRoute` | **活动 UI 未使用** | `discovery.reloadRoute` | `unused`；设计文档要求保留“各范围重载”，建议补 UI |
| 35 | `GET /discovery/backends/` | `discoveryBackends` | 仅死代码 `BackendForm` | `discovery.backends` | `unused`；建议恢复后端发现 |

### 4.2 AI Gateway（base `/ai-gateway`，独立服务）

| # | 方法/路径 | 前端函数 | 使用位置 | 新 contract | 状态/备注 |
| --- | --- | --- | --- | --- | --- |
| 36 | `GET /v1/admin/plugins/{plugin}/schema` | `getAiGatewayPluginSchema` | AI 队列宿主配置 | `aiQueue.schema` | `P1-contract` |
| 37 | `GET /v1/admin/plugins/{plugin}/readme` | `getAiGatewayPluginReadme` | AI 队列文档页签 | `aiQueue.readme` | `P1-contract`；旧 UI 原样 `<pre>`，不渲染 Markdown |
| 38 | `GET /v1/admin/tenant-rate-limits?tenant&model&path&policy` | `listTenantRateLimits` | 租户配额列表 | `aiQueue.rules` | `P1-contract` |
| 39 | `PUT /v1/admin/tenant-rate-limits` body=规则 | `upsertTenantRateLimit` | 租户配额保存 | `aiQueue.upsert` | `P1-contract` |
| 40 | `DELETE /v1/admin/tenant-rate-limits` body=规则 | `deleteTenantRateLimit` | 租户配额删除 | `aiQueue.delete` | `P1-contract`；注意**DELETE 带 JSON body**，代理/小程序请求层需支持 |
| 41 | `GET /v1/admin/tenant-rate-limits/resolve?...` | `resolveTenantRateLimit` | 解析当前请求 | `aiQueue.resolve` | `P1-contract` |

### 4.3 明确缺失/需扩展的点

1. **按插件 code 列实例**：`plugin.list` 没有 code 过滤，而旧 UI 有 5 处依赖 `GET /config/plugins/{code}`（插件中心、插件选择、AI 队列）。这是 P1 必须补齐的 contract/上游适配能力，否则只能全量拉取，Wasm/原生实例数量大时行为和性能都会变。
2. **Wasm schema 预览缺 HTTP 头**：`SpacegatePluginPreviewWasmSchemaInputSchema` 缺 `httpHeaders`，无法预览私有 HTTP(S) 制品；同时旧 UI 也没有“未保存预览”按钮，迁移时若实现预览需同时补 contract 和 UI。
3. **路由级重载无 UI**：contract 已有 `discovery.reloadRoute`，旧 UI 未用；设计文档要求“各范围重载”，应补。
4. **后端发现无 UI**：contract 有 `discovery.backends`，活动 UI 未用；旧 `BackendForm` 是唯一入口，迁移时应恢复或明确移除。
5. **网关/路由删除影响提示**：contract 只有删除，不返回影响面；旧 UI 用前端已知路由数做提示。新实现若需要精确提示，需额外查询或服务端返回摘要。
6. **插件属性缓存刷新**：contract 有 `plugin.refresh`，但旧页面“刷新配置”不会调用；新实现应明确“刷新本地视图”与“刷新服务端插件缓存”的区别。
7. **AI 队列 schema/readme 无版本**：contract 输出无 version，符合只读；但新服务仍需处理 AI Gateway 的 401/超时/文本错误，并避免把上游 token 暴露给客户端。

---

### 4.4 请求/响应形状（实现 contract 时直接对照）

> 所有 admin-server 路径都相对于 `/api`（开发代理会去掉 `/api`）。成功响应通常是 JSON；`POST/PUT/DELETE` 成功体为空，失败体是纯文本 `Internal error: ...`（`admin-server/src/error.rs:14-19`）。插件创建/更新是特例：**query 携带实例 ID，body 只放 spec JSON**。

#### 4.4.1 config / gateway / route

| 操作 | 请求 | 响应 | 证据 |
| --- | --- | --- | --- |
| 配置项名称 | `GET /config/names` | `string[]` | `dist/api/index.js:114-118` |
| 读取网关 | `GET /config/item/{configItemName}/gateway` | `SgGateway \| null` | `config.rs:64-69`、`dist/api/index.js:89-93` |
| 创建网关 | `POST /config/item/{configItemName}/gateway`，body=`SgGateway` | 空 | `config.rs:126-132`、`dist/api/index.js:153-157` |
| 更新网关 | `PUT /config/item/{configItemName}/gateway`，body=`SgGateway` | 空 | `config.rs:153-159`、`dist/api/index.js:172-176` |
| 删除网关 | `DELETE /config/item/{configItemName}/gateway` | 空 | `config.rs:205-207`、`dist/api/index.js:207-211` |
| 路由名称 | `GET /config/item/{gateway}/route/names` | `string[]` | `config.rs:76-81` |
| 全部路由 | `GET /config/item/{gateway}/route/all` | `Record<string, SgRoute>` | `config.rs:82-87` |
| 读取路由 | `GET /config/item/{gateway}/route/item/{route}` | `SgRoute \| null` | `config.rs:70-75` |
| 创建路由 | `POST /config/item/{gateway}/route/item/{route}`，body=`SgRoute` | 空 | `config.rs:133-139` |
| 更新路由 | `PUT /config/item/{gateway}/route/item/{route}`，body=`SgRoute` | 空 | `config.rs:161-167` |
| 路由改名 | `POST /config/item/{gateway}/route/item/{oldRoute}/rename`，body=`SgRoute`（`route_name` 必须是新名） | 空 | `config.rs:169-177`、`service.rs:145-162` |
| 删除路由 | `DELETE /config/item/{gateway}/route/item/{route}` | 空 | `config.rs:209-214` |

注意：

- 网关路径参数是 **config item 目录名**，不一定等于 `SgGateway.name`；旧 UI 用 `configItemNames` 映射（`Gateways.vue:20-23`、`127-132`）。
- 路由路径参数是文件名，`SgRoute.route_name` 是 JSON 字段；rename 要求两者目标一致（`service.rs:149-151`）。
- 创建网关/路由在目标已存在时返回 500 文本 `Internal error: item existed` / `route existed`，不是 409（`fs/create.rs:26-46`）。

#### 4.4.2 plugin

| 操作 | 请求 | 响应 | 证据 |
| --- | --- | --- | --- |
| 全部实例 | `GET /config/plugin-all` | `PluginConfig[]` | `config.rs:97-99` |
| 按 ID 读取 | `GET /config/plugin?code=&kind=&name=`（named）或 `&uid=`（anon） | `PluginConfig \| null` | `config.rs:100-105`、`dist/api/index.js:129-133` |
| 按 code 列表 | `GET /config/plugins/{code}` | `PluginConfig[]` | `config.rs:107-112` |
| 创建实例 | `POST /config/plugin?code=&kind=&name=&display_name=`，body=`spec`（任意 JSON） | 空 | `config.rs:140-149`、`dist/api/index.js:163-167` |
| 更新实例 | `PUT /config/plugin?code=&kind=&name=&display_name=`，body=`spec` | 空 | `config.rs:191-200`、`dist/api/index.js:198-202` |
| 删除实例 | `DELETE /config/plugin?code=&kind=&name=`（或 `uid`） | 空 | `config.rs:224-230`、`dist/api/index.js:227-231` |
| 插件目录 | `GET /plugin/list` | `string[]` | `plugin.rs:177-180` |
| 全部属性 | `GET /plugin/attr-all` | `PluginAttributes[]` | `plugin.rs:181-184` |
| 单个属性 | `GET /plugin/attr/{code}` | `PluginAttributes \| null` | `plugin.rs:185-188` |
| 插件 schema | `GET /plugin/schema/{code}` | JSON Schema 或 `null` | `plugin.rs:98-104` |
| 已保存 Wasm schema | `GET /plugin/wasm/schema?code=&kind=&name=` | JSON Schema；缺失文件返回 `{}` | `plugin.rs:106-114`、`120-130` |
| 未保存 Wasm schema | `POST /plugin/wasm/schema/preview`，body=`{image_url, schema_path?, oci_auth?, http_headers?}` | JSON Schema | `plugin.rs:82-92`、`116-118`；旧前端函数 `api/aiGateway.ts:96-101` |
| 刷新属性缓存 | `POST /plugin/refresh` | `PluginAttributes[]` | `plugin.rs:189-192` |

`display_name` 进入 query 而不是 body；空/空白会被服务端规范化为 `None`（`config.rs:22-40`、`275-293`）。SDK 的 `pluginUpsertAsQuery` 只在非空时发送（`dist/api/index.js:35-44`）。这意味着**前端不能通过 body 传展示名**，迁移时若改成 JSON body 需要改上游接口或适配器。

#### 4.4.3 auth / discovery

| 操作 | 请求 | 响应 | 证据 |
| --- | --- | --- | --- |
| 登录 | `POST /auth/login`，body=`{ak, sk}` | 空；`Set-Cookie: jwt=...; HttpOnly; Max-Age=3600` | `auth.rs:16-51` |
| 实例列表 | `GET /discovery/instance/list` | `string[]` | `discovery.rs:121-124` |
| 健康 | `GET /discovery/instance/health` | `Record<string, boolean>` | `discovery.rs:112-119` |
| 后端发现 | `GET /discovery/backends/`（注意 SDK 末尾斜杠） | `BackendHost[]` | `discovery.rs:126-128`、`dist/api/index.js:294-298` |
| 网关重载 | `POST /discovery/instance/reload/gateway?instance=&gateway=` | 空 | `discovery.rs:155-172` |
| 全局重载 | `POST /discovery/instance/reload/global?instance=` | 空 | `discovery.rs:174-191` |
| 路由重载 | `POST /discovery/instance/reload/route?instance=&gateway=&route=` | 空 | `discovery.rs:193-213` |

重载失败：找不到实例返回 500 文本 `instance not found`；推送事件失败返回 `fail to transfer request, status: ..., message: ...`（`discovery.rs:99-109`、`155-213`）。

#### 4.4.4 AI Gateway

| 操作 | 请求 | 响应 | 证据 |
| --- | --- | --- | --- |
| 插件 schema | `GET /v1/admin/plugins/{plugin}/schema` | JSON Schema | `api/aiGateway.ts:79-81` |
| 插件 readme | `GET /v1/admin/plugins/{plugin}/readme`，`Accept: text/markdown` | `text/markdown` 原文 | `api/aiGateway.ts:103-109` |
| 规则列表 | `GET /v1/admin/tenant-rate-limits?tenant=&model=&path=&policy=` | `TenantRateLimitRuleView[]` | `api/aiGateway.ts:111-120` |
| 规则 upsert | `PUT /v1/admin/tenant-rate-limits`，body=`TenantRateLimitRule` | 单条 `TenantRateLimitRuleView` | `api/aiGateway.ts:132-137` |
| 规则删除 | `DELETE /v1/admin/tenant-rate-limits`，body=`TenantRateLimitRule` | `{deleted:number}` | `api/aiGateway.ts:139-144` |
| 规则解析 | `GET /v1/admin/tenant-rate-limits/resolve?tenant=&model=&path=&policy=` | `TenantRateLimitResolution` | `api/aiGateway.ts:122-130` |

AI Gateway 的 `request()` 只根据 `response.ok` 和 `content-type` 判断：非 2xx 时把响应文本作为 `Error`；JSON 则 `response.json()`；否则 `response.text()`（`api/aiGateway.ts:60-77`）。没有超时、重试、401 刷新、版本头。

#### 4.4.5 版本与错误矩阵

| 场景 | HTTP | 响应头/体 | 客户端行为 | 证据 |
| --- | --- | --- | --- | --- |
| GET 成功 | 200 | `X-Server-Version: <n>` + JSON | 更新本地 `clientVersion` | `version_control.rs:51-56`、`dist/api/index.js:62-74` |
| 写成功 | 200 | 无 `X-Server-Version`，空体 | 本地版本不变，依赖后续 GET | `version_control.rs:32-57` |
| 版本不匹配的写 | 409 | `X-Server-Version: <latest>`，空体 | 抛 `ExceptionVersionConflict`；提示刷新；不自动重试 | `version_control.rs:42-49`、`dist/api/index.js:75-83`、`main.ts:63-66` |
| 未认证 | 401 | 文本 `expect jwt token` / `invalid jwt token` | 抛 `ExceptionUnauthorized`；弹登录框 | `authentication.rs:35-39`、`dist/api/index.js:75-85` |
| 业务/IO 错误 | 500 | 文本 `Internal error: ...` | AxiosError 分支 `ElMessage.warning(data)` 或页面通用失败 | `error.rs:14-19`、`main.ts:60-62` |
| 只读 POST | 200 | 正常响应 | `/plugin/wasm/schema/preview`、`/plugin/refresh` 不参与版本控制 | `version_control.rs:63`、`71-90` |

---

## 5. 跨页依赖与状态传播

### 5.1 依赖图

```text
URL query gatewayName
  ├─ 顶栏 ConsoleGatewaySelect（写回 query）
  ├─ Overview（当前网关详情 + 路由；指标不随切换刷新）
  ├─ Routes（切网关 → 重新拉路由）
  └─ Plugins（切网关 → 重新拉当前网关，用于 Wasm 全局开关）

Instances（独立 selectedGateway，仅用于“网关重载”）
  └─ 不读 query，不与顶栏同步

Gateway 保存/删除
  ├─ 影响 Routes（文件后端级联删除；K8s 后端路由可能残留）
  ├─ 影响 Plugins 的 Wasm 全局启用状态
  └─ 影响 Instances 的网关重载目标列表

Plugin 实例保存/删除
  ├─ 影响所有引用它的 Gateway/Route/Rule/Backend 绑定
  ├─ 影响 PluginSelect / PluginListForm 的下拉与标签
  └─ 删除后绑定仍保留，详情显示“实例不存在”

Route 保存/改名/删除
  ├─ 影响 Overview 路由概况
  ├─ 影响 Gateway 列表的路由计数
  └─ 不清理 Wasm 绑定配置（可能留下孤儿）

插件中心 Wasm 定义保存
  ├─ 同步所有 binding_base_plugin 相同的绑定配置（逐个 PUT）
  └─ 显示“需要全局重载”

任何配置写操作
  └─ 不自动重载实例；需到 Instances 手动重载
```

### 5.2 刷新/失效规则（旧实现）

| 事件 | 会刷新什么 | 不会刷新什么 |
| --- | --- | --- |
| 顶栏切网关 | Overview 详情/路由、Routes、Plugins 当前网关 | 顶栏网关选项、Instances 选择、Overview 指标 |
| 网关保存/删除 | 网关列表、路由计数 | 顶栏网关选项、路由页、插件页、实例页 |
| 路由保存/删除 | 当前网关路由列表 | 网关路由计数、概览（除非重新进入） |
| 插件实例保存/删除 | 当前插件的实例列表、绑定下拉 | 其他打开中的表单、资源绑定列表（除非重新挂载） |
| Wasm 定义保存 | Wasm 卡片、相关绑定配置（服务端） | 运行实例（需重载） |
| AI 宿主保存 | 队列实例 | 运行实例（需重载） |
| 实例重载 | 无 | 实例列表/健康、配置版本视图 |
| 页面“刷新配置” | 重建插件面板 | 服务端插件属性缓存 |

---

## 6. 回归检查表（旧操作 → 未来平台）

说明：

- `共享`：Web/App/Desktop 可复用同一套共享 Svelte 组件与 store；平台差异只在入口、token、文件/剪贴板/拖拽能力。
- `原生`：小程序必须用 WXML/TS 原生页面实现，不能依赖 DOM、Monaco、HTML5 drag、`window.open`、`location.assign`。
- `定制表单`：该操作包含不能用通用 JSON 文本框替代的专用交互。
- 所有行都是**必须保留**的能力；`备注` 写平台适配点。

### 6.1 全局

| # | 操作 | 旧证据 | Web | App | Desktop | 小程序 | 定制表单 | 备注 |
| --- | --- | --- | --- | --- | --- | --- | --- | --- |
| G1 | 侧栏导航 + 折叠 | `App.vue:31-38`、`112-129` | 共享 | 共享 | 共享 | 原生 | 否 | 小程序用 tabBar/抽屉 |
| G2 | 顶栏网关选择/刷新/管理 | `ConsoleGatewaySelect.vue:37-84` | 共享 | 共享 | 共享 | 原生 | 否 | 小程序 picker + 刷新按钮 |
| G3 | 实例健康 5s 轮询 | `App.vue:78-98` | 共享 | 共享 | 共享 | 原生 | 否 | 小程序注意后台定时器与请求频率 |
| G4 | 语言切换 zh/en | `App.vue:40-43` | 共享 | 共享 | 共享 | 原生 | 否 | 需决定 es 是否保留；统一 locale key |
| G5 | 主题 | 无 | — | — | — | — | 否 | 旧版没有；新框架可提供，但不属于回归项 |
| G6 | 401 登录弹窗 ak/sk | `main.ts:41-59` | 共享 | 共享 | 共享 | 原生 | 是（登录表单） | 新架构改 Hai IAM；旧 ak/sk 仅迁移期 |
| G7 | 409 冲突提示 | `main.ts:63-66` | 共享 | 共享 | 共享 | 原生 | 否 | 必须提示“刷新后重试”，不能自动重试 |
| G8 | 剪贴板复制/读取 | `utils/index.ts:100-184` | 共享 | 原生适配 | 共享 | 原生 | 否 | 小程序用 `wx.setClipboardData/getClipboardData` |
| G9 | 文件下载/上传 JSON | `GatewayForm.vue:38-53` | 共享 | 原生适配 | 共享 | 原生 | 否 | 小程序用 `wx.chooseMessageFile`/`wx.saveFile`，或改为文本导入导出 |
| G10 | 拖拽排序 | `PluginListForm.vue:115-126` | 共享 | 原生适配 | 共享 | 原生 | 是（排序） | 小程序用上/下移按钮，不能依赖 HTML5 drag |
| G11 | `window.open` 插件主页/仓库 | `PluginPanel.vue:584-590` | 共享 | 原生适配 | 共享 | 原生 | 否 | 小程序需复制链接或 web-view 白名单 |
| G12 | `location.assign('/instances')` | `PluginPanel.vue:612-614` | 共享 | 共享 | 共享 | 原生 | 否 | 小程序用 `wx.navigateTo` |

### 6.2 概览

| # | 操作 | 旧证据 | Web | App | Desktop | 小程序 | 定制表单 | 备注 |
| --- | --- | --- | --- | --- | --- | --- | --- | --- |
| O1 | 查看四项指标 | `Overview.vue:183-204` | 共享 | 共享 | 共享 | 原生 | 否 | 明确“插件配置”含绑定配置 |
| O2 | 点击指标跳转 | `184-203` | 共享 | 共享 | 共享 | 原生 | 否 | 保留 gatewayName |
| O3 | 当前网关摘要 | `206-221` | 共享 | 共享 | 共享 | 原生 | 否 | 监听/插件/Redis |
| O4 | 实例健康表 | `223-239` | 共享 | 共享 | 共享 | 原生 | 否 | 空态/未知态 |
| O5 | 路由概况前 6 条 | `242-267` | 共享 | 共享 | 共享 | 原生 | 否 | 匹配/后端摘要逻辑要复用纯函数 |
| O6 | AI 治理说明 | `269-277` | 共享 | 共享 | 共享 | 原生 | 否 | 静态文案 |

### 6.3 网关

| # | 操作 | 旧证据 | Web | App | Desktop | 小程序 | 定制表单 | 备注 |
| --- | --- | --- | --- | --- | --- | --- | --- | --- |
| W1 | 网关列表/刷新 | `Gateways.vue:119-145`、`239` | 共享 | 共享 | 共享 | 原生 | 否 | 避免 N+1，新服务聚合 |
| W2 | 新建网关 | `147-153`、`83-117` | 共享 | 共享 | 共享 | 原生 | 是（完整网关表单） | 名称 + 默认监听 |
| W3 | 编辑网关（名称只读） | `155-161`、`GatewayForm.vue:124-126` | 共享 | 共享 | 共享 | 原生 | 是 | 明确是否支持改运行时名称；旧版不支持 |
| W4 | 删除网关（含级联提示） | `182-196` | 共享 | 共享 | 共享 | 原生 | 否 | 需显示路由数影响 |
| W5 | 监听器增删 | `GatewayForm.vue:22-36`、`242-254` | 共享 | 共享 | 共享 | 原生 | 是 | 至少一个？旧版无保存校验 |
| W6 | 监听器 IP/端口/协议/主机名 | `ListenerForm.vue:56-84` | 共享 | 共享 | 共享 | 原生 | 是 | 端口 0-65535 |
| W7 | TLS mode/key/cert | `ListenerForm.vue:85-92` | 共享 | 共享 | 共享 | 原生 | 是 | 大文本 key/cert；小程序注意输入长度 |
| W8 | TLS http2 | 无 UI | 共享 | 共享 | 共享 | 原生 | 是 | 旧版只能 JSON；迁移应补显式开关 |
| W9 | Redis URL + 4 个连接池参数 | `GatewayForm.vue:137-187` | 共享 | 共享 | 共享 | 原生 | 是 | 单位 ms/连接数；仅 redis_url 有值时显示 |
| W10 | 日志级别/语言 | `189-217` | 共享 | 共享 | 共享 | 原生 | 是 | 下拉枚举 |
| W11 | x-request-id / ignore TLS | `218-229` | 共享 | 共享 | 共享 | 原生 | 是 | 注意 null/false 语义 |
| W12 | 网关级插件绑定 | `264-268` | 共享 | 共享 | 共享 | 原生 | 是 | 优先级/拖拽/实例选择 |
| W13 | 网关 JSON 导入/导出 | `278-309` | 共享 | 原生适配 | 共享 | 原生 | 是（文本 JSON） | 保留“编辑模式强制保留名称” |
| W14 | 未保存关闭确认 | `209-223` | 共享 | 共享 | 共享 | 原生 | 否 | 修正“取消”绕过问题 |
| W15 | 可观测性配置 | 模型有、UI 无 | 共享 | 共享 | 共享 | 原生 | 是 | 旧版缺失；若要保留需补表单 |

### 6.4 路由

| # | 操作 | 旧证据 | Web | App | Desktop | 小程序 | 定制表单 | 备注 |
| --- | --- | --- | --- | --- | --- | --- | --- | --- |
| R1 | 路由列表/搜索 | `Routes.vue:91-101`、`163-179` | 共享 | 共享 | 共享 | 原生 | 否 | 搜索路由/主机/后端 |
| R2 | 新建 HTTP/MCP | `181-186`、`RouteEditorDrawer.vue:273-282` | 共享 | 共享 | 共享 | 原生 | 是 | 类型切换会重置模型，迁移应确认后再切 |
| R3 | 编辑路由 | `188-193` | 共享 | 共享 | 共享 | 原生 | 是 | 完整抽屉 |
| R4 | 复制路由 | `195-201` | 共享 | 共享 | 共享 | 原生 | 是 | 名称 `-copy`，需处理冲突 |
| R5 | 改名并保存 | `213-214`、`service.rs:145-162` | 共享 | 共享 | 共享 | 原生 | 是 | 非原子；冲突/404 语义 |
| R6 | 删除路由 | `228-241` | 共享 | 共享 | 共享 | 原生 | 否 | 确认；建议提示绑定影响 |
| R7 | hostnames 开关/增删 | `284-289`、`406-414` | 共享 | 共享 | 共享 | 原生 | 是 | `null` vs `[]` |
| R8 | 多规则/多匹配 | `416-451`、`797-918` | 共享 | 共享 | 共享 | 原生 | 是 | 折叠面板、规则删除至少保留一条 |
| R9 | 路径 kind/value/rewrite | `828-845` | 共享 | 共享 | 共享 | 原生 | 是 | Prefix/Exact/RegExp |
| R10 | 方法多选 | `847-851` | 共享 | 共享 | 共享 | 原生 | 是 | 7 个方法 |
| R11 | Header 匹配/重写 | `871-883` | 共享 | 共享 | 共享 | 原生 | 是 | exact/reg_exp |
| R12 | Query 匹配 | `895-906` | 共享 | 共享 | 共享 | 原生 | 是 | exact/regular |
| R13 | 规则级 timeout/timeout_mode/balance | 无 UI | 共享 | 共享 | 共享 | 原生 | 是 | 旧版只能 JSON；迁移应补 |
| R14 | 后端增删 | `997-1003`、`1077-1083` | 共享 | 共享 | 共享 | 原生 | 是 | HTTP 可 0 个；MCP 至少 1 个 |
| R15 | 后端 Host/K8s/File | `933-968` | 共享 | 共享 | 共享 | 原生 | 是 | 切换 kind 重置 host |
| R16 | 后端端口/协议/权重/超时/HTTP2 | `970-993` | 共享 | 共享 | 共享 | 原生 | 是 | 端口 1-65535；权重≥0 |
| R17 | 后端插件 | `995-1001`、`1075-1081` | 共享 | 共享 | 共享 | 原生 | 是 | 每个后端独立 |
| R18 | MCP transport/path/SSE | `747-795` | 共享 | 共享 | 共享 | 原生 | 是 | 两套路径 |
| R19 | MCP timeout/session affinity | `777-791` | 共享 | 共享 | 共享 | 原生 | 是 | 枚举 |
| R20 | 路由/规则插件链 | `1089-1114` | 共享 | 共享 | 共享 | 原生 | 是 | 三层插件 |
| R21 | 快速示例 | `694-699`、`510-563` | 共享 | 共享 | 共享 | 原生 | 是 | 建议保留 |
| R22 | 高级 JSON 复制/读取/应用 | `1116-1129` | 共享 | 原生适配 | 共享 | 原生 | 是（文本 JSON） | 不要用 Monaco 作为唯一入口 |
| R23 | 保存校验 | `565-603` | 共享 | 共享 | 共享 | 原生 | 是 | 路径/端口/权重 |
| R24 | 未保存关闭确认 | `655-661` | 共享 | 共享 | 共享 | 原生 | 否 | 修正“取消”绕过 |
| R25 | 后端发现选择器 | `BackendForm.vue:32-67`（死代码） | 共享 | 共享 | 共享 | 原生 | 是 | 活动 UI 已丢失；建议恢复 |

### 6.5 插件

| # | 操作 | 旧证据 | Web | App | Desktop | 小程序 | 定制表单 | 备注 |
| --- | --- | --- | --- | --- | --- | --- | --- | --- |
| P1 | 原生插件目录/搜索 | `PluginPanel.vue:632-682` | 共享 | 共享 | 共享 | 原生 | 否 | 卡片列表 |
| P2 | 原生插件元数据/主页/仓库 | `772-788`、`584-590` | 共享 | 共享 | 共享 | 原生 | 否 | 外链适配 |
| P3 | 原生实例列表/搜索 | `791-815` | 共享 | 共享 | 共享 | 原生 | 否 | 仅 named |
| P4 | 创建原生实例（schema 表单/JSON） | `464-521`、`PluginForm.vue:206-274` | 共享 | 共享 | 共享 | 原生 | 是 | Monaco 仅 Web/Desktop；小程序用原生表单/文本 JSON |
| P5 | 编辑原生实例 | `516-521` | 共享 | 共享 | 共享 | 原生 | 是 | 同上 |
| P6 | 删除原生实例 | `522-539` | 共享 | 共享 | 共享 | 原生 | 否 | 确认 + 引用影响提示 |
| P7 | 原生 mono 实例管理 | 旧版缺失 | 共享 | 共享 | 共享 | 原生 | 是 | 需补，不然 mono 无法管理 |
| P8 | Wasm 定义卡片/排序 | `PluginPanel.vue:252-264`、`wasmPlugin.ts:97-127` | 共享 | 共享 | 共享 | 原生 | 否 | priority 降序 |
| P9 | Wasm 全局启停 | `PluginPanel.vue:361-405` | 共享 | 共享 | 共享 | 原生 | 是（开关） | PUT 网关 plugins |
| P10 | 添加/编辑自定义 Wasm | `407-420`、`ThirdPartyWasmDrawer.vue:301-554` | 共享 | 共享 | 共享 | 原生 | 是 | 5 个页签、JSON 大文本、密码字段 |
| P11 | 删除 Wasm 定义 | `541-560` | 共享 | 共享 | 共享 | 原生 | 否 | 确认 + 重载提示 |
| P12 | Wasm 绑定选择 | `PluginSelect.vue:233-254` | 共享 | 共享 | 共享 | 原生 | 是 | 基础配置下拉 |
| P13 | Wasm 绑定方式 default/schema/yaml | `PluginSelect.vue:702-752` | 共享 | 共享 | 共享 | 原生 | 是 | xml 代码支持但 UI 未提供 |
| P14 | 从镜像读取 schema 生成表单 | `PluginSelect.vue:392-409` | 共享 | 共享 | 共享 | 原生 | 是 | 依赖 admin-server 读 OCI/HTTP/file |
| P15 | 绑定优先级 | `PluginListForm.vue:214-226` | 共享 | 共享 | 共享 | 原生 | 是 | int32；默认 0 |
| P16 | 绑定拖拽排序 | `PluginListForm.vue:115-126` | 共享 | 原生适配 | 共享 | 原生 | 是 | 小程序用上/下移 |
| P17 | 绑定详情/运行时配置 | `PluginBindingDetail.vue:70-144`、`PluginRuntimeConfigEditor.vue:163-206` | 共享 | 共享 | 共享 | 原生 | 是 | schema/json/yaml/xml |
| P18 | 展示名称编辑 | `PluginBindingDetail.vue:102-105`、`PluginForm.vue:207-209` | 共享 | 共享 | 共享 | 原生 | 是 | 注意改的是共享实例 |
| P19 | 插件刷新 | `Plugins.vue:26-28` | 共享 | 共享 | 共享 | 原生 | 否 | 需区分本地/服务端缓存 |
| P20 | schema 表单（嵌套对象/数组） | `SchemaForm.vue:135-192` | 共享 | 共享 | 共享 | 原生 | 是 | 深度≤3；enum/oneOf 需补 |
| P21 | schema JSON 字段 | `SchemaJsonField.vue:33-44` | 共享 | 共享 | 共享 | 原生 | 是 | 文本 JSON |
| P22 | OCI/HTTP(S) 认证输入 | `ThirdPartyWasmDrawer.vue:493-546` | 共享 | 共享 | 共享 | 原生 | 是 | 密码掩码；不要日志泄露 |

### 6.6 AI 队列

| # | 操作 | 旧证据 | Web | App | Desktop | 小程序 | 定制表单 | 备注 |
| --- | --- | --- | --- | --- | --- | --- | --- | --- |
| A1 | 宿主配置打开/保存 | `AiQueueRateLimit.vue:75-101`、`AiGatewayQueueDrawer.vue:214-229` | 共享 | 共享 | 共享 | 原生 | 是 | Wasm URL/服务地址/VM 池 |
| A2 | 宿主 schema 表单 | `AiGatewayQueueDrawer.vue:310-318` | 共享 | 共享 | 共享 | 原生 | 是 | AI Gateway schema |
| A3 | 文档页签 | `323-333` | 共享 | 共享 | 共享 | 原生 | 否 | 旧版 `<pre>` 原文 |
| A4 | 队列观测空态 | `334-336` | 共享 | 共享 | 共享 | 原生 | 否 | 预留 |
| A5 | 配额列表/过滤 | `TenantRateLimitTable.vue:373-432` | 共享 | 共享 | 共享 | 原生 | 是（筛选表单） | 固定高 520 |
| A6 | 配额新增/编辑 | `435-513` | 共享 | 共享 | 共享 | 原生 | 是 | 内联编辑器，不是弹窗 |
| A7 | 配额删除 | `325-345` | 共享 | 共享 | 共享 | 原生 | 否 | 确认 |
| A8 | Redis key 预览 | `172-183`、`479-482` | 共享 | 共享 | 共享 | 原生 | 否 | 纯函数复用 |
| A9 | 解析当前请求 | `299-323`、`484-507` | 共享 | 共享 | 共享 | 原生 | 是（结果面板） | 命中/回退/候选 key |
| A10 | TTL 永久/有限 | `475-478` | 共享 | 共享 | 共享 | 原生 | 是 | 旧版清除能力弱，需改进 |

### 6.7 实例与登录

| # | 操作 | 旧证据 | Web | App | Desktop | 小程序 | 定制表单 | 备注 |
| --- | --- | --- | --- | --- | --- | --- | --- | --- |
| I1 | 实例列表/健康 | `Instances.vue:70-85`、`170-188` | 共享 | 共享 | 共享 | 原生 | 否 | 未知态 |
| I2 | 复制实例 ID | `134-141` | 共享 | 原生适配 | 共享 | 原生 | 否 | 剪贴板 |
| I3 | 网关重载 | `113-132` | 共享 | 共享 | 共享 | 原生 | 否 | 建议加确认 |
| I4 | 全局重载 | `99-111` | 共享 | 共享 | 共享 | 原生 | 否 | 建议加确认 |
| I5 | 路由级重载 | 旧版缺失 | 共享 | 共享 | 共享 | 原生 | 是（选网关+路由） | 后端已有接口 |
| I6 | 登录 ak/sk | `Login.vue:18-31` | 共享 | 共享 | 共享 | 原生 | 是 | 新架构 Hai IAM；旧入口仅兼容期 |
| I7 | 登录错误提示 | 旧版缺失 | 共享 | 共享 | 共享 | 原生 | 否 | 必须补 |

---

## 7. 已知遗留怪癖与未提交工作特性

### 7.1 未提交/工作树特性（不得丢失）

> 本次按要求未执行 git，无法逐行确认“已提交/未提交”。下列特性在**当前工作树**中真实存在，且设计文档 93 行明确把它们纳入验收基线；迁移时必须以工作树行为为准，而不是假设上游 `main` 或外层 `spacegate-admin-front` 已有这些能力。

| 特性 | 证据 | 迁移要求 |
| --- | --- | --- |
| 路由改名保留完整配置/后端 | `src/views/Routes.vue:213-214`、`sdk/api/index.js:182-187`、`admin-server/src/service/config.rs:169-177`、`crates/config/src/service.rs:145-162` | 必须保留；注意非原子、冲突/404 错误 |
| Redis 连接池 4 参数 | `GatewayForm.vue:147-187`、`tests/gateway-redis-pool-config.test.mjs:7-24` | 必须保留单位、默认值、仅 redis_url 有值时显示 |
| 插件展示名称 `display_name` | `PluginForm.vue:25-30`、`PluginBindingDetail.vue:102-105`、`admin-server/src/service/config.rs:22-40` | 顶层字段；不进入实例 ID；改共享实例会影响所有引用 |
| 网关配置项 ID 与运行时名称分离 | `src/views/Gateways.vue:20-23`、`127-132`、`203-205`、`tests/gateway-config-item-identity.test.mjs:7-15` | 必须保留“config item id 用于路径/删除，name 用于展示”的模型 |
| 路由抽屉规则/后端编辑 + JSON | `RouteEditorDrawer.vue:1-1138`、`tests/route-editor-drawer.test.mjs:7-15` | HTTP 允许 0 后端；MCP 至少 1 后端 |
| Schema `allOf/anyOf/oneOf` 解析 | `utils/schemaEditor.ts:36-70`、`tests/schema-editor-resolution.test.mjs:15-58` | 必须保留解析语义；但 enum 渲染需补 |
| Wasm OCI/HTTP/local 来源与认证头 | `ThirdPartyWasmDrawer.vue:46-76`、`81-86`、`202-223`、`493-546` | 保留协议、认证字段、掩码；不要只留 image_url |
| Wasm 镜像 schema 读取/预览 | `api/aiGateway.ts:92-101`、`admin-server/src/service/plugin.rs:106-147` | 已保存实例 schema 可用；未保存预览 API 存在但 UI 未接 |
| 插件中心全局启停 + 重载提示 | `PluginPanel.vue:361-405`、`690-704` | 保留开关、网关级 PUT、重载引导 |
| AI 租户配额 CRUD + 解析 | `TenantRateLimitTable.vue:250-345`、`api/aiGateway.ts:111-144` | 保留 key 预览、resolve、TTL |
| 绑定优先级/拖拽 | `PluginListForm.vue:115-126`、`214-226` | 优先级 int32；相同优先级拖拽顺序 |

### 7.2 怪癖与缺陷清单（迁移时“保留能力、修正实现”）

| # | 怪癖 | 影响 | 证据 | 建议 |
| --- | --- | --- | --- | --- |
| Q1 | 抽屉“取消”直接 `drawerOpen=false`，绕过 `before-close` | 未保存内容丢失且无确认 | `Gateways.vue:285`、`RouteEditorDrawer.vue:1134`、`216-223`、`655-661` | 取消也走统一确认 |
| Q2 | 网关表单注释说可改运行时名称，但编辑模式 `readonly` | 无法重命名网关 | `Gateways.vue:22`、`GatewayForm.vue:125` | 明确产品决策；若要支持需后端 rename |
| Q3 | 路由类型 HTTP↔MCP 切换整体重置模型 | 误切会丢失已填内容 | `RouteEditorDrawer.vue:273-282` | 切换前确认，或保留可映射字段 |
| Q4 | HTTP 规则级 timeout/timeout_mode/balance_policy 无 UI | 只能 JSON 编辑，普通用户不可见 | `RouteEditorDrawer.vue:920-1087`、`343-345` | 补控件 |
| Q5 | 网关 observability 无 UI | 只能 JSON 导入 | `Gateways.vue:96-104`、`GatewayForm.vue` 无字段 | 补表单或明确不支持 |
| Q6 | 监听器 TLS `http2` 无 UI | 只能 JSON 编辑 | `ListenerForm.vue:40-45`、`85-92` | 补开关 |
| Q7 | 高级 JSON “应用”无结构校验 | 可写入缺字段模型，后续渲染报错 | `RouteEditorDrawer.vue:609-617`、`GatewayForm.vue:42-53` | 加 Zod/JSON Schema 校验 |
| Q8 | 路由创建/复制无重名检查 | 后端 500，提示不明确 | `Routes.vue:203-226`、`fs/create.rs:37-46` | 前端预检 + 冲突错误码 |
| Q9 | 路由 rename 非原子 | 失败可能新旧并存 | `service.rs:145-162` | 服务端事务/补偿 |
| Q10 | 路由改名/删除不清理 Wasm 绑定配置 | 孤儿配置 | `wasmPlugin.ts:351-394`、`Routes.vue:228-241` | 明确级联或清理任务 |
| Q11 | 路由插件重复限制只在 route 范围 | rule/backend/gateway 可重复 code | `PluginListForm.vue:99-109` | 统一策略或明确允许 |
| Q12 | 点击 tag 关闭立即本地删除，无确认 | 误删绑定 | `PluginListForm.vue:168-175` | 加确认/撤销 |
| Q13 | 绑定详情改展示名会改共享实例 | 影响其他资源 | `PluginBindingDetail.vue:46-65` | UI 明示“全局改名” |
| Q14 | mono 插件在插件中心无法列/编辑 | 功能缺失 | `PluginPanel.vue:789-791`、`791-842` | 补 mono 实例页 |
| Q15 | PluginSelect 自定义保存固定 `kind:'named'` | mono 插件创建错误 | `PluginSelect.vue:461-483`、`PluginForm.vue:146-153` | 按 attr.mono 写 kind |
| Q16 | 插件属性缓存 1 小时，页面刷新不强制刷新 | 新插件/删除不即时可见 | `admin-server/src/service/plugin.rs:26`、`Plugins.vue:26-28` | 提供“刷新服务端缓存”或缩短 TTL |
| Q17 | Wasm 绑定复制 `oci_auth`/`http_headers` | 敏感信息多份存储 | `wasmPlugin.ts:351-394` | 绑定只存引用，不复制 secret；或服务端加密 |
| Q18 | 未保存预览 API 未接 + contract 缺 httpHeaders | 无法预览私有 HTTP 制品 | `api/aiGateway.ts:96-101`、`spacegate-schemas.ts:162-166` | 补 contract + UI |
| Q19 | 写成功不返回版本，靠后续 GET 同步 | 连续写易 409 | `version_control.rs:32-57`、`dist/api/index.js:62-74` | 按设计文档 P1：写后服务端重新读取版本并显式返回 |
| Q20 | reload POST 推进版本 | 重载导致其他标签页冲突 | `version_control.rs:59-65`、`discovery.rs:155-213` | 重载应只读语义，或单独版本域 |
| Q21 | 409 后客户端版本不更新 | 必须刷新整页 | `dist/api/index.js:75-83`、`main.ts:63-66` | 冲突错误携带 serverVersion，前端刷新快照 |
| Q22 | 登录失败静默且仍 reload | 可能循环弹窗 | `Login.vue:8-14`、`main.ts:54-57` | 显示错误、不 reload |
| Q23 | `en-US` 与消息 key `en` 不一致 | 英文可能回退失败 | `App.vue:40-43`、`locales/index.ts:5-10` | 统一 locale key + fallback |
| Q24 | es 文案不完整且不可选 | 第三种语言形同虚设 | `locales/es.ts:2-6`、`65-110` | 要么补全并暴露，要么移除 |
| Q25 | 无主题切换 | 设计文档只要求语言/主题共享层，旧版无 | `App.vue:105-161` | 新框架可新增，不算回归缺失 |
| Q26 | AI Gateway fetch 绕过 SDK/401/版本 | 认证/错误语义不一致 | `api/aiGateway.ts:60-77` | 由服务端代理，统一错误码 |
| Q27 | `.env` 的 `VITE_API_BASE_PATH='admin'` 与 `aiGateway.ts` 默认 `/api` 冲突 | 开发构建可能把 schema 请求发到相对 `admin/...` | `.env:1`、`api/aiGateway.ts:52-58` | 统一环境变量名与值 |
| Q28 | DELETE 携带 JSON body | 某些代理/小程序请求层会丢 body | `api/aiGateway.ts:139-144` | 改为 path/query 标识或 POST |
| Q29 | TTL `clearable` 用在 input-number | 无法清空为永久 | `TenantRateLimitTable.vue:475-478` | 显式永久开关 |
| Q30 | 两处 TenantRateLimitTable 独立状态 | 数据不同步 | `AiQueueRateLimit.vue:94`、`AiGatewayQueueDrawer.vue:321` | 共享 store 或只保留一处 |
| Q31 | 策略下拉英文界面仍中文 | i18n 不一致 | `TenantRateLimitTable.vue:378-381`、`455-458` | 统一 i18n key |
| Q32 | “取消”重置为新增而非恢复 | 编辑误操作丢原值 | `TenantRateLimitTable.vue:511` | 恢复快照 |
| Q33 | 路由页无刷新按钮 | 只能切网关/重进 | `Routes.vue:253-261` | 补刷新 |
| Q34 | 概览切网关不刷新指标 | 数据可能陈旧 | `Overview.vue:169` | 统一切换刷新 |
| Q35 | 顶栏刷新不校正失效选择 | 可能保留已删除网关 | `ConsoleGatewaySelect.vue:54-56`、`79-81` | 刷新后校验并回退 |
| Q36 | 实例 ID 直接拼 `http://` | 依赖 ID 为 host:port | `Instances.vue:179-183` | 服务端返回 api_url |
| Q37 | 实例页网关选择与顶栏不同步 | 重载目标易错 | `Instances.vue:19`、`73-74` | 共享选择器或明确独立 |
| Q38 | 全局重载无确认 | 误点影响全量配置 | `Instances.vue:201-208` | 加确认/二次确认 |
| Q39 | 后端发现入口丢失 | 用户无法从已发现后端选择 | `BackendForm.vue:32-67`、活动抽屉无引用 | 恢复或明确移除 |
| Q40 | 概览“插件配置”含绑定配置 | 指标语义误导 | `Overview.vue:128-135` | 明确“实例配置”还是“插件类型” |
| Q41 | 插件删除无成功提示 | 用户不确定是否成功 | `PluginPanel.vue:522-539` | 统一 toast |
| Q42 | 保存失败多处只显示通用文案 | 排障困难 | `Gateways.vue:177-179`、`Routes.vue:221-223` | 透传错误码/文本 |
| Q43 | 删除取消靠字符串比较 | Element Plus 版本差异风险 | `Gateways.vue:192-195`、`Routes.vue:237-240` | 用 Promise reject 类型判断 |
| Q44 | `Api.getClient()` 依赖 `self` 全局 | 非浏览器环境不可用 | `dist/api/index.js:16-21` | 新客户端显式实例化 |
| Q45 | Monaco schema URI 写死 `http:///api/...` | 依赖代理/协议 | `hooks/index.ts:8-22` | 用 contract 客户端或去掉 Monaco 依赖 |

---

## 8. 最高风险迁移项（按风险排序）

1. **Contract 能力缺口：按 code 查插件实例。** 旧 UI 的插件中心、插件选择器、AI 队列都依赖 `GET /config/plugins/{code}`；P1 contract 的 `plugin.list` 没有 code 过滤。若不补，迁移后要么全量拉取（行为/性能变化），要么在 serv 端做隐式过滤（contract 不显式）。这是最容易在“页面能打开”之后才暴露的缺口。
2. **Wasm 绑定复制 secret 的模型。** `buildBoundWasmPluginConfig` 用 `...baseSpec` 把 `oci_auth`、`http_headers` 复制到每个资源绑定配置；旧 UI 只做显示掩码。迁移若继续复制，会把私有仓库密码/Authorization 分散到大量配置文件；若改为引用，又必须兼容旧绑定字段。需要在 P1/P3 明确数据模型和迁移/脱敏策略。
3. **版本冲突语义与重载副作用。** 旧实现“写成功不返回版本、靠后续 GET 同步”“reload POST 推进版本”“409 后必须整页刷新”。设计文档要求新服务显式传递版本、写后重新读取、409 映射为冲突且禁止自动重试。任何一端（client/store/service/上游适配器）偷懒都会造成连续写失败或静默丢更新。必须用并发测试和双标签页场景固定。
4. **专用表单的保真度。** 旧版有大量不能退化为 JSON 的交互：网关监听器/TLS/Redis 池、路由匹配/重写/多后端、MCP 传输、插件 schema 表单、Wasm 五页签抽屉、租户限流内联编辑器、绑定优先级/拖拽。设计文档明确禁止通用 JSON 替代。尤其小程序没有 Monaco、HTML5 drag、DOM，必须逐项做原生等价交互。
5. **Schema 表单语义不完整。** 旧版 `enum` 不渲染下拉、`oneOf` 仅扁平化标量、嵌套深度≤3、数字禁止负数、必填不递归。迁移若照搬“有 schema 就自动表单”，用户会遇到“枚举变文本框、复杂变体变 JSON、负数填不了”。需要用插件真实 schema 建立快照测试，明确哪些字段回退 JSON。
6. **路由身份与生命周期。** `config item id`/运行时 `name` 分离、路由 rename 非原子、删除/改名不清理 Wasm 绑定、网关无法改名。新 contract 的 `route.rename` 只保证“先建后删”，没有事务；UI 必须处理“旧路由已删/新路由已建/两者都在”的中间态。
7. **跨页失效与重载。** 旧版保存后不自动重载，靠提示和手动实例页；切网关只刷新部分页面；插件缓存 1 小时。新共享 store 如果做成“全局响应式自动刷新”，可能改变语义；如果完全照旧，又会出现陈旧数据。需要明确“保存→刷新列表→提示重载→实例页执行”的状态机。
8. **AI Gateway 旁路。** 旧版 schema/readme/租户规则直接 `fetch('/ai-gateway')`，不带 SDK 版本/401 处理，同源时可能携带 admin JWT Cookie，且 DELETE 带 body。新架构应由 serv 代理并统一鉴权/错误码，同时保留 readme 原文、schema 表单、resolve 结果和 TTL 语义。
9. **i18n 与平台能力。** `en-US` vs `en`、es 不完整、无主题、无持久化；小程序还需要 `wx` 剪贴板/文件/导航/定时器适配。迁移前应确定 locale 清单（zh-CN/en-US/是否保留 es）和回退策略，避免“英文界面一半 key”。
10. **缺失的旧能力。** 后端发现选择器（`/discovery/backends`）和路由级重载（`/discovery/.../reload/route`）在活动 UI 中已丢失；设计文档要求保留“各范围重载”和既有交互，建议在 P3 作为显式补回项，而不是当作新功能。

---

## 9. 迁移验收建议（直接可执行）

1. **逐页对照本文件第 2 节**，在 Web 先实现完整闭环，再用同一 contract/store 接 App/Desktop；小程序按第 6 节“原生”列逐项实现。
2. **为纯函数建立快照测试**：`routeMatchSummary`/`backendSummary`/`hostnameSummary`（`console-utils.ts:3-76`）、`pluginInstanceDisplayName`/`pluginConfigOptionName`（`pluginInstance.ts:28-56`）、`resolveSchema`/`schemaFieldEditorMode`/`isSchemaJsonValue`（`schemaEditor.ts:36-103`）、`buildBoundWasmPluginConfig`/`buildThirdPartyWasmPluginConfig`/`refreshBoundWasmPluginSource`（`wasmPlugin.ts:309-468`）、Redis key 预览（`TenantRateLimitTable.vue:172-183`）。旧工程已有部分 Node 测试可作输入：`tests/*.test.mjs`、`components/config/src/utils/*.test.ts`。
3. **为版本语义建立集成测试**：首次 GET 取得版本 → 写成功 → 服务端返回新版本（按设计文档，不允许依赖客户端再 GET）→ 409 携带 serverVersion → 前端提示刷新且不自动重试 → 重载不再推进配置版本域。
4. **为专用表单建立 e2e**：网关 TLS/Redis、HTTP 多规则多后端、MCP SSE、Wasm 五页签、schema enum/oneOf/嵌套数组、租户配额 resolve、实例重载。Playwright 只覆盖 Web/App/Desktop 预览；小程序用 mock `wx` 覆盖同一业务流程，不能以浏览器预览代替。
5. **对每个写操作验证“取消/关闭/失败/409”四条路径**，尤其是抽屉取消绕过确认（Q1）和登录失败循环（Q22）。
6. **对敏感字段做日志/产物扫描**：`oci_auth.password`、`bearer_token`、`identity_token`、`http_headers`、`sk`、JWT、LDAP bind 密码不得进入 `PUBLIC_`/`VITE_`、客户端日志、构建产物或审计明文。

---

## 10. 一句话总结

旧管理端的核心价值不在页面数量，而在**一套围绕 gatewayName 的配置编辑状态机 + 三层插件绑定 + Wasm 双形态（定义/绑定）+ 版本冲突语义**。迁移的最大风险不是“页面画不出来”，而是把专用表单退化成 JSON、把版本冲突做成自动重试、把 Wasm 认证信息复制得到处都是，以及丢掉路由改名、Redis 池、展示名称这些已在工作树中的能力，和后端发现、路由级重载这些已存在于旧代码/后端但活动 UI 缺失的能力。
