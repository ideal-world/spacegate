---
name: hai-api-contract
description: "新增、组合或修改共享 HTTP 接口的路径与输入输出 Schema，供 serv 与 typed client 共用。"
---

# hai-api-contract

## 能力契约

| 项目 | 契约 |
| --- | --- |
| 能力 | 使用 @h-ai/api-contract 定义、组合和扩展共享 HTTP contract 与 Zod 输入输出 |
| 适用场景 | 新增/修改共享接口定义，不负责实现 handler |
| 输入 | 领域路径、schema、HTTP 方法、共享错误模型 |
| 输出 | 供 serv 实现和 api-client 消费的 contract |
| 限制 | 纯定义，无 init/close；不包含服务端实现或密钥。业务路径属于 contract，公共传输配置属于 transport。 |

## 使用路径

1. 查当前公开 exports 与已有领域 contract，确认业务路径唯一来源。
2. 在 contract 中声明 method/path 与输入输出 Schema；只导出真正跨接口或跨层复用的 Schema。
3. 用 `apiContract.create` 组合领域；不把服务实现、私有配置或传输前缀写进 contract。
4. 同步 serv procedures、typed client、调用方与失败/校验测试；普通业务输出按当前 HaiResult 契约。
5. 独立 contract 包实际执行声明构建。推导需要的上游公共类型 `ContractProcedureBuilderWithInputOutput`、`ContractProcedureBuilderWithOutput`、`Schema` 已由本包转出；显式类型从公开入口 `import type`，不引用依赖内部路径。

## 按需参考

按本次操作读取对应小节；以已安装版本的公开类型核验签名，示例中的业务变量由应用提供。

- [运行环境](reference.md#运行环境)
- [适用场景](reference.md#适用场景)
- [使用步骤](reference.md#使用步骤)
- [核心 API](reference.md#核心-api)
- [Contract 定义规范](reference.md#contract-定义规范)
- [常见模式](reference.md#常见模式)
- [测试](reference.md#测试)
