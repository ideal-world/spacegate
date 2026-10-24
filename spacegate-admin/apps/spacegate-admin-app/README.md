# spacegate-admin-app

## 简介

App/H5 前端工程，复用 shared 的响应式 mini 后台，使用 Svelte 5 + Vite + Capacitor，可输出 Web 预览并同步 Android / iOS 原生工程。

## 核心功能清单

- 响应式 H5 页面与 Capacitor Android/iOS 原生壳。
- 共享管理界面、个人设置和接口客户端。
- 原生使用安全 TokenStorage，浏览器预览使用 Cookie 刷新。

## 使用入门

### i18n 与 shared 协同

- 本端页面文案：`messages/{locale}.json` + `project.inlang/settings.json`
- 共享 Shell/主题/语言/API：`packages/spacegate-admin-shared`
- 语言切换通过 shared runtime 与本端 `src/lib/paraglide/runtime.js` 同步。
- 个人设置提供头像、语言、风格与改密，管理员可打开用户、角色与审计日志页面。原生端使用 Capacitor 安全 TokenStorage；浏览器预览使用 httpOnly refresh Cookie。

自定义建议：移动端独有引导、运营位、按钮文案放在本目录 `messages/`；跨端通用组件放到 shared。

### 编译、打包与部署

```bash
# 开发预览，默认端口 5175
pnpm --filter spacegate-admin-app dev

# 编译静态 SPA 产物到 dist/
pnpm --filter spacegate-admin-app build

# 同步原生工程；首次使用前按需执行 cap:add:android / cap:add:ios
pnpm --filter spacegate-admin-app package

# Android / iOS 原生打包
pnpm --filter spacegate-admin-app cap:build:android:debug
pnpm --filter spacegate-admin-app cap:build:android:release
pnpm --filter spacegate-admin-app cap:build:ios
```

本地开发通过根 `.env` 的 `PUBLIC_API_BASE` 配置代理。原生打包前，在本端 `public/runtime-config.js` 中设置 `globalThis.__HAI_PUBLIC_CONFIG__ = { apiBase: 'https://api.example.com/api/v1' }`，再编译并同步原生工程；这里只能放公开地址，不能放密钥。Android 模拟器访问宿主机通常使用 `http://10.0.2.2:3000/api/v1`。

### IAM 认证配置

服务端 `config/_iam.yml` 统一控制登录、注册、重置、协议与 `ui` 品牌。邮件/短信可用 `_reach.yml` 中的 Provider/标准模板或自定义发送回调。`AuthPanel` 使用安全公开配置及框架认证动作；无需在页面适配 Token、验证码倒计时和错误提示。单实例可使用 Memory；多实例需共享数据库与 Redis，并按工程根 README 配置写入路由和传输会话。

LDAP/Reach 均可选：基础认证无需安装。默认 LDAP 客户端需在 IAM 服务端 workspace 安装 ldapts，再配置 login.ldap 与连接信息。自定义 onOtpSendEmail/onOtpSendSms/onPasswordResetRequest 回调可独立于 Reach 使用，投递失败须拒绝 Promise；回调第三参数 context 提供 expiresAt/idempotencyKey，发送服务按稳定键持久化去重，异常/超时为结果未知且不自动重发。协议在 Modal 显示；LoginForm/RegisterForm 提供 header/extraFields/footer，AuthPanel 透传 loginHeader/loginFields/loginFooter 和 registerHeader/registerFields/registerFooter。配置及例子见生成项目的 hai-iam/hai-ui skills。

协议支持配置中的 userAgreementMarkdown/privacyPolicyMarkdown 正文及 URL；同主题优先 Markdown，由安全渲染器在弹窗显示。

完整契约见生成应用的 `.agents/skills/hai-iam/reference.md`。

## 开发测试

从工程根执行 `pnpm typecheck`、`pnpm lint`、`pnpm build`、`pnpm test` 和 `pnpm test:e2e`。浏览器预览通过后，原生壳仍需在对应系统或设备单独验收。

## 许可证

Apache-2.0
