# spacegate-admin-miniapp

## 简介

小程序预留目录，目前没有可运行的小程序应用。

## 核心功能清单

- 已提供 `project.inlang/settings.json` 与中英文 `messages/`。
- 可复用 `packages/spacegate-admin-contract` 的接口类型。

## 使用入门

选定小程序框架后再实现页面、请求适配和认证。小程序文案放本目录 `messages/`，跨端通用内容放 `packages/spacegate-admin-shared`；消息格式插件使用工程根已安装的依赖。

## 开发测试

本目录不参与 workspace 构建、单元测试或 E2E。接入实际框架后增加对应命令，并在目标平台验证登录、请求、导航与发布包。

## 许可证

Apache-2.0
