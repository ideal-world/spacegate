# spacegate-admin-desktop

## 简介

桌面端工程，参考 `apps/desktop-app` 使用 Tauri v2 + Svelte 5 + Vite，前端通过共享 contract 调用 `spacegate-admin-serv`。

## 核心功能清单

- Tauri v2 桌面壳与共享管理界面。
- 运行时配置 API 地址，无需按环境重复打包。
- 原生令牌仅存内存，重启后重新登录。

## 使用入门

### i18n 与 shared 协同

- 本端页面文案：`messages/{locale}.json` + `project.inlang/settings.json`
- 共享 Shell/主题/语言/API：`packages/spacegate-admin-shared`
- 个人设置保存语言与风格偏好到 IAM 元数据，并同步 shared 和本端 Paraglide runtime

原生 Tauri 使用内存 TokenStorage，不将凭证写入磁盘；重启后需要重新登录。浏览器预览使用 httpOnly refresh Cookie。两种方式均经共享加密 API 客户端。`src-tauri/tauri.conf.json` 的资源与网络白名单应与部署的 API 地址同步。

自定义建议：桌面端菜单、状态、快捷操作文案放在本目录 `messages/`；跨端通用能力放到 shared。

### 编译、打包与部署

```bash
# 仅前端开发，默认端口 5176
pnpm --filter spacegate-admin-desktop dev

# Tauri 桌面开发
pnpm --filter spacegate-admin-desktop tauri:dev

# 编译 WebView 前端产物到 dist/
pnpm --filter spacegate-admin-desktop build

# 在工程根打包桌面安装包；在对应系统构建 windows / linux / macos
pnpm build --target desktop --platform windows --arch x64
```

打包前需要安装 Rust 工具链和 Tauri 平台依赖。构建不读取 .env.prod/.env.local；启动已安装程序前设置进程 PUBLIC_API_BASE（HTTP(S)，不含凭据、查询和片段）。Rust 会在页面加载前注入公开地址，仅将该 Origin 加入 CSP；不需要为 test/prod 重打安装包。缺省连接 http://localhost:3000/api/v1。

### IAM 认证配置

服务端 `config/_iam.yml` 统一控制登录、注册、重置、协议与 `ui` 品牌。邮件/短信可用 `_reach.yml` 中的 Provider/标准模板或自定义发送回调。`AuthPanel` 使用安全公开配置及框架认证动作；无需在页面适配 Token、验证码倒计时和错误提示。单实例可使用 Memory；多实例需共享数据库与 Redis，并按工程根 README 配置写入路由和传输会话。

LDAP/Reach 均可选：基础认证无需安装。默认 LDAP 客户端需在 IAM 服务端 workspace 安装 ldapts，再配置 login.ldap 与连接信息。自定义 onOtpSendEmail/onOtpSendSms/onPasswordResetRequest 回调可独立于 Reach 使用，投递失败须拒绝 Promise；回调第三参数 context 提供 expiresAt/idempotencyKey，发送服务按稳定键持久化去重，异常/超时为结果未知且不自动重发。协议在 Modal 显示；LoginForm/RegisterForm 提供 header/extraFields/footer，AuthPanel 透传 loginHeader/loginFields/loginFooter 和 registerHeader/registerFields/registerFooter。配置及例子见生成项目的 hai-iam/hai-ui skills。

协议支持配置中的 userAgreementMarkdown/privacyPolicyMarkdown 正文及 URL；同主题优先 Markdown，由安全渲染器在弹窗显示。

完整契约见生成应用的 `.agents/skills/hai-iam/reference.md`。

## 开发测试

从工程根执行 `pnpm typecheck`、`pnpm lint`、`pnpm build`、`pnpm test` 和 `pnpm test:e2e`。浏览器预览通过后，原生壳仍需在对应系统或设备单独验收。

## 许可证

Apache-2.0
