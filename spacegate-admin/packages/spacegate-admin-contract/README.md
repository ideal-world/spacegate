# spacegate-admin-contract

## 简介

共享 API 契约包，包含 Zod Schema、`apiContract.route()` 业务接口定义与工程级 contract。业务 schema、角色、流程及限额是模板初始方案，按用户需求修改，并同步服务端授权、调用方、测试与说明；不要求需求沿用默认业务。

## 核心功能清单

- 共享接口路径、输入输出 Schema 和类型。
- 客户端与服务端使用同一份 contract。
- 不包含服务端业务实现，也不加载私有配置。

## 使用入门

### 骨架说明

- `src/app-schemas.ts`：跨接口、跨层复用的输入/数据 Schema 与类型。
- `src/app-contract.ts`：app 领域 contract；每个业务 path 直接写在对应接口定义中。
- `src/admin-schemas.ts` / `src/admin-contract.ts`：示例管理员用户、初始 admin/user 角色、分页审计、个人设置、加密 JSON 头像上传 Schema/类型/路由；认证复用框架的 IAM auth contract，通过服务端配置启用注册、密码/LDAP/验证码登录及密码重置。
- `src/spacegate-admin-contract.ts`：工程级 contract，供 `@h-ai/serv` 与 `@h-ai/api-client` 共用。

只使用一次的输出 Schema 直接内联到 `.output(...)`；同一 contract 文件内重复使用时才保留私有常量。
API 前缀、健康检查与跨端 Header 等传输配置位于 serv 包的 `config/_serv.yml`。
`SUPPORTED_LOCALES` 为生成时实际编译的语言列表；`app.info` 中名称、版本、环境、默认语言来自后端 core 配置。更新公开契约时同时更新 serv、shared typed API client、各端 UI 与测试。

### 开发

- `pnpm --filter spacegate-admin-contract typecheck`
- `pnpm --filter spacegate-admin-contract test`

## 开发测试

```sh
pnpm --filter spacegate-admin-contract typecheck
pnpm --filter spacegate-admin-contract test
```

## 许可证

Apache-2.0
