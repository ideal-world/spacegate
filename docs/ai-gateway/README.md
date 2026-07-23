# AI Gateway 文档入口

`ai-gateway-queue` 是随 SpaceGate 镜像发布的系统 Wasm 插件。生产环境通过 `SgFilter` CRD 挂载，不应在通用路由插件下拉列表中作为可手工创建、可手工绑定的插件实例出现。

- [生产部署与 CRD 配置](../k8s/production-deployment.md)
- [AI Gateway 测试规格](test-spec.md)
- [Wasm 插件实现与配置](../../plugins/wasm/ai-gateway-queue/README.md)

历史的手工插件实例配置流程已移至 [归档](../archive/ai-gateway/admin-ui-guide-legacy.md)，仅用于追溯旧实现。
