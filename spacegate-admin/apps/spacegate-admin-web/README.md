# spacegate-admin-web

## 简介

响应式 mini 后台 Web 前端，通过 `packages/spacegate-admin-shared` 调用后端 API，并通过 `@h-ai/ui` 组件搭建页面。

## 核心功能清单

- 浏览器访问共享管理界面与个人设置。
- 与 API 共用接口类型、登录状态、主题和语言。
- 可单独构建 Web 镜像并增量更新。

## 使用入门

### i18n 与 shared 协同

- 本端页面文案：`messages/{locale}.json` + `project.inlang/settings.json`
- 共享 Shell/主题/语言/API：`packages/spacegate-admin-shared`
- 个人设置保存语言与风格偏好到 IAM 元数据，并同步 shared 和本端 Paraglide runtime

自定义建议：Web 独有文案放在本目录 `messages/`；跨端通用文案或组件放到 shared。

### 编译、打包与部署

```bash
# 开发预览，默认端口 4173
pnpm --filter spacegate-admin-web dev

# 编译 Web 前端
pnpm --filter spacegate-admin-web build
```

从工程根运行 `pnpm build --target docker` 导出镜像。默认带部署工具，可运行 `pnpm run deploy --env test --build` 一次完成构建发布，或 `pnpm run deploy --env prod --bundle-only` 交付已有镜像及跨平台脚本。完整包包含自建中间件，人工部署与按服务更新见工程根 README。

### IAM 认证配置

服务端 `config/_iam.yml` 统一控制登录、注册、重置、协议与 `ui` 品牌。邮件/短信可用 `_reach.yml` 中的 Provider/标准模板或自定义发送回调。`AuthPanel` 使用安全公开配置及框架认证动作；无需在页面适配 Token、验证码倒计时和错误提示。单实例可使用 Memory；多实例需共享数据库与 Redis，并按工程根 README 配置写入路由和传输会话。

LDAP/Reach 均可选：基础认证无需安装。默认 LDAP 客户端需在 IAM 服务端 workspace 安装 ldapts，再配置 login.ldap 与连接信息。自定义 onOtpSendEmail/onOtpSendSms/onPasswordResetRequest 回调可独立于 Reach 使用，投递失败须拒绝 Promise；回调第三参数 context 提供 expiresAt/idempotencyKey，发送服务按稳定键持久化去重，异常/超时为结果未知且不自动重发。协议在 Modal 显示；LoginForm/RegisterForm 提供 header/extraFields/footer，AuthPanel 透传 loginHeader/loginFields/loginFooter 和 registerHeader/registerFields/registerFooter。配置及例子见生成项目的 hai-iam/hai-ui skills。

协议支持配置中的 userAgreementMarkdown/privacyPolicyMarkdown 正文及 URL；同主题优先 Markdown，由安全渲染器在弹窗显示。

完整契约见生成应用的 `.agents/skills/hai-iam/reference.md`。

## 开发测试

从工程根执行 `pnpm typecheck`、`pnpm lint`、`pnpm build`、`pnpm test` 和 `pnpm test:e2e`。浏览器预览通过后，原生壳仍需在对应系统或设备单独验收。

## 许可证

Apache-2.0
