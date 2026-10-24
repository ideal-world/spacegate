import { apiContract } from '@h-ai/api-contract'
import {
  SpacegateAiQueueDeleteOutputSchema,
  SpacegateAiQueuePluginInputSchema,
  SpacegateAiQueueReadmeOutputSchema,
  SpacegateAiQueueResolveInputSchema,
  SpacegateAiQueueResolveOutputSchema,
  SpacegateAiQueueRuleInputSchema,
  SpacegateAiQueueRuleOutputSchema,
  SpacegateAiQueueRulesInputSchema,
  SpacegateAiQueueRulesOutputSchema,
  SpacegateAiQueueSchemaOutputSchema,
  SpacegateConfigGetOutputSchema,
  SpacegateConfigNameInputSchema,
  SpacegateConfigUpdateInputSchema,
  SpacegateDiscoveryHealthOutputSchema,
  SpacegateDiscoveryReloadGatewayInputSchema,
  SpacegateDiscoveryReloadGlobalInputSchema,
  SpacegateDiscoveryReloadRouteInputSchema,
  SpacegateEmptyInputSchema,
  SpacegateGatewayDeleteInputSchema,
  SpacegateGatewayGetInputSchema,
  SpacegateGatewayGetOutputSchema,
  SpacegateGatewayUpsertInputSchema,
  SpacegateNamesOutputSchema,
  SpacegatePluginAttrOutputSchema,
  SpacegatePluginCodeInputSchema,
  SpacegatePluginDeleteInputSchema,
  SpacegatePluginGetInputSchema,
  SpacegatePluginGetOutputSchema,
  SpacegatePluginListInputSchema,
  SpacegatePluginPreviewWasmSchemaInputSchema,
  SpacegatePluginUpsertInputSchema,
  SpacegatePluginWasmSchemaInputSchema,
  SpacegateResourceListOutputSchema,
  SpacegateRouteDeleteInputSchema,
  SpacegateRouteGetInputSchema,
  SpacegateRouteGetOutputSchema,
  SpacegateRouteListInputSchema,
  SpacegateRouteListOutputSchema,
  SpacegateRouteRenameInputSchema,
  SpacegateRouteUpsertInputSchema,
  SpacegateSchemaOutputSchema,
  SpacegateSchemaPreviewOutputSchema,
  SpacegateVersionOutputSchema,
} from './spacegate-schemas.js'

/**
 * SpaceGate 管理面上游 contract。
 *
 * 所有路由都是 serv RPC 风格的 POST；input/output 使用 Zod 校验，
 * 上游 Rust admin-server 的 REST 路径、鉴权和版本头由服务端适配器处理。
 * 写操作必须显式携带 `version`，不能由服务端替换成全局最新版本。
 */
export const spacegateContract = {
  config: {
    names: apiContract.route({ method: 'POST', path: '/spacegate/config/names', operationId: 'spacegate.config.names', tags: ['spacegate'] })
      .input(SpacegateEmptyInputSchema)
      .output(apiContract.haiResultSchema(SpacegateNamesOutputSchema)),
    get: apiContract.route({ method: 'POST', path: '/spacegate/config/get', operationId: 'spacegate.config.get', tags: ['spacegate'] })
      .input(SpacegateConfigNameInputSchema)
      .output(apiContract.haiResultSchema(SpacegateConfigGetOutputSchema)),
    update: apiContract.route({ method: 'POST', path: '/spacegate/config/update', operationId: 'spacegate.config.update', tags: ['spacegate'] })
      .input(SpacegateConfigUpdateInputSchema)
      .output(apiContract.haiResultSchema(SpacegateVersionOutputSchema)),
  },
  gateway: {
    get: apiContract.route({ method: 'POST', path: '/spacegate/gateway/get', operationId: 'spacegate.gateway.get', tags: ['spacegate'] })
      .input(SpacegateGatewayGetInputSchema)
      .output(apiContract.haiResultSchema(SpacegateGatewayGetOutputSchema)),
    upsert: apiContract.route({ method: 'POST', path: '/spacegate/gateway/upsert', operationId: 'spacegate.gateway.upsert', tags: ['spacegate'] })
      .input(SpacegateGatewayUpsertInputSchema)
      .output(apiContract.haiResultSchema(SpacegateVersionOutputSchema)),
    delete: apiContract.route({ method: 'POST', path: '/spacegate/gateway/delete', operationId: 'spacegate.gateway.delete', tags: ['spacegate'] })
      .input(SpacegateGatewayDeleteInputSchema)
      .output(apiContract.haiResultSchema(SpacegateVersionOutputSchema)),
  },
  route: {
    list: apiContract.route({ method: 'POST', path: '/spacegate/route/list', operationId: 'spacegate.route.list', tags: ['spacegate'] })
      .input(SpacegateRouteListInputSchema)
      .output(apiContract.haiResultSchema(SpacegateRouteListOutputSchema)),
    get: apiContract.route({ method: 'POST', path: '/spacegate/route/get', operationId: 'spacegate.route.get', tags: ['spacegate'] })
      .input(SpacegateRouteGetInputSchema)
      .output(apiContract.haiResultSchema(SpacegateRouteGetOutputSchema)),
    upsert: apiContract.route({ method: 'POST', path: '/spacegate/route/upsert', operationId: 'spacegate.route.upsert', tags: ['spacegate'] })
      .input(SpacegateRouteUpsertInputSchema)
      .output(apiContract.haiResultSchema(SpacegateVersionOutputSchema)),
    rename: apiContract.route({ method: 'POST', path: '/spacegate/route/rename', operationId: 'spacegate.route.rename', tags: ['spacegate'] })
      .input(SpacegateRouteRenameInputSchema)
      .output(apiContract.haiResultSchema(SpacegateVersionOutputSchema)),
    delete: apiContract.route({ method: 'POST', path: '/spacegate/route/delete', operationId: 'spacegate.route.delete', tags: ['spacegate'] })
      .input(SpacegateRouteDeleteInputSchema)
      .output(apiContract.haiResultSchema(SpacegateVersionOutputSchema)),
  },
  plugin: {
    list: apiContract.route({ method: 'POST', path: '/spacegate/plugin/list', operationId: 'spacegate.plugin.list', tags: ['spacegate'] })
      .input(SpacegatePluginListInputSchema)
      .output(apiContract.haiResultSchema(SpacegateResourceListOutputSchema)),
    get: apiContract.route({ method: 'POST', path: '/spacegate/plugin/get', operationId: 'spacegate.plugin.get', tags: ['spacegate'] })
      .input(SpacegatePluginGetInputSchema)
      .output(apiContract.haiResultSchema(SpacegatePluginGetOutputSchema)),
    upsert: apiContract.route({ method: 'POST', path: '/spacegate/plugin/upsert', operationId: 'spacegate.plugin.upsert', tags: ['spacegate'] })
      .input(SpacegatePluginUpsertInputSchema)
      .output(apiContract.haiResultSchema(SpacegateVersionOutputSchema)),
    delete: apiContract.route({ method: 'POST', path: '/spacegate/plugin/delete', operationId: 'spacegate.plugin.delete', tags: ['spacegate'] })
      .input(SpacegatePluginDeleteInputSchema)
      .output(apiContract.haiResultSchema(SpacegateVersionOutputSchema)),
    catalog: apiContract.route({ method: 'POST', path: '/spacegate/plugin/catalog', operationId: 'spacegate.plugin.catalog', tags: ['spacegate'] })
      .input(SpacegateEmptyInputSchema)
      .output(apiContract.haiResultSchema(SpacegateNamesOutputSchema)),
    attrAll: apiContract.route({ method: 'POST', path: '/spacegate/plugin/attr-all', operationId: 'spacegate.plugin.attrAll', tags: ['spacegate'] })
      .input(SpacegateEmptyInputSchema)
      .output(apiContract.haiResultSchema(SpacegateResourceListOutputSchema)),
    attr: apiContract.route({ method: 'POST', path: '/spacegate/plugin/attr', operationId: 'spacegate.plugin.attr', tags: ['spacegate'] })
      .input(SpacegatePluginCodeInputSchema)
      .output(apiContract.haiResultSchema(SpacegatePluginAttrOutputSchema)),
    schema: apiContract.route({ method: 'POST', path: '/spacegate/plugin/schema', operationId: 'spacegate.plugin.schema', tags: ['spacegate'] })
      .input(SpacegatePluginCodeInputSchema)
      .output(apiContract.haiResultSchema(SpacegateSchemaOutputSchema)),
    wasmSchema: apiContract.route({ method: 'POST', path: '/spacegate/plugin/wasm-schema', operationId: 'spacegate.plugin.wasmSchema', tags: ['spacegate'] })
      .input(SpacegatePluginWasmSchemaInputSchema)
      .output(apiContract.haiResultSchema(SpacegateSchemaOutputSchema)),
    previewWasmSchema: apiContract.route({ method: 'POST', path: '/spacegate/plugin/preview-wasm-schema', operationId: 'spacegate.plugin.previewWasmSchema', tags: ['spacegate'] })
      .input(SpacegatePluginPreviewWasmSchemaInputSchema)
      .output(apiContract.haiResultSchema(SpacegateSchemaPreviewOutputSchema)),
    refresh: apiContract.route({ method: 'POST', path: '/spacegate/plugin/refresh', operationId: 'spacegate.plugin.refresh', tags: ['spacegate'] })
      .input(SpacegateEmptyInputSchema)
      .output(apiContract.haiResultSchema(SpacegateResourceListOutputSchema)),
  },
  discovery: {
    instances: apiContract.route({ method: 'POST', path: '/spacegate/discovery/instances', operationId: 'spacegate.discovery.instances', tags: ['spacegate'] })
      .input(SpacegateEmptyInputSchema)
      .output(apiContract.haiResultSchema(SpacegateNamesOutputSchema)),
    health: apiContract.route({ method: 'POST', path: '/spacegate/discovery/health', operationId: 'spacegate.discovery.health', tags: ['spacegate'] })
      .input(SpacegateEmptyInputSchema)
      .output(apiContract.haiResultSchema(SpacegateDiscoveryHealthOutputSchema)),
    reloadGateway: apiContract.route({ method: 'POST', path: '/spacegate/discovery/reload-gateway', operationId: 'spacegate.discovery.reloadGateway', tags: ['spacegate'] })
      .input(SpacegateDiscoveryReloadGatewayInputSchema)
      .output(apiContract.haiResultSchema(SpacegateVersionOutputSchema)),
    reloadGlobal: apiContract.route({ method: 'POST', path: '/spacegate/discovery/reload-global', operationId: 'spacegate.discovery.reloadGlobal', tags: ['spacegate'] })
      .input(SpacegateDiscoveryReloadGlobalInputSchema)
      .output(apiContract.haiResultSchema(SpacegateVersionOutputSchema)),
    reloadRoute: apiContract.route({ method: 'POST', path: '/spacegate/discovery/reload-route', operationId: 'spacegate.discovery.reloadRoute', tags: ['spacegate'] })
      .input(SpacegateDiscoveryReloadRouteInputSchema)
      .output(apiContract.haiResultSchema(SpacegateVersionOutputSchema)),
    backends: apiContract.route({ method: 'POST', path: '/spacegate/discovery/backends', operationId: 'spacegate.discovery.backends', tags: ['spacegate'] })
      .input(SpacegateEmptyInputSchema)
      .output(apiContract.haiResultSchema(SpacegateResourceListOutputSchema)),
  },
  aiQueue: {
    schema: apiContract.route({ method: 'POST', path: '/spacegate/ai-queue/schema', operationId: 'spacegate.aiQueue.schema', tags: ['spacegate'] })
      .input(SpacegateAiQueuePluginInputSchema)
      .output(apiContract.haiResultSchema(SpacegateAiQueueSchemaOutputSchema)),
    readme: apiContract.route({ method: 'POST', path: '/spacegate/ai-queue/readme', operationId: 'spacegate.aiQueue.readme', tags: ['spacegate'] })
      .input(SpacegateAiQueuePluginInputSchema)
      .output(apiContract.haiResultSchema(SpacegateAiQueueReadmeOutputSchema)),
    rules: apiContract.route({ method: 'POST', path: '/spacegate/ai-queue/rules', operationId: 'spacegate.aiQueue.rules', tags: ['spacegate'] })
      .input(SpacegateAiQueueRulesInputSchema)
      .output(apiContract.haiResultSchema(SpacegateAiQueueRulesOutputSchema)),
    upsert: apiContract.route({ method: 'POST', path: '/spacegate/ai-queue/upsert', operationId: 'spacegate.aiQueue.upsert', tags: ['spacegate'] })
      .input(SpacegateAiQueueRuleInputSchema)
      .output(apiContract.haiResultSchema(SpacegateAiQueueRuleOutputSchema)),
    delete: apiContract.route({ method: 'POST', path: '/spacegate/ai-queue/delete', operationId: 'spacegate.aiQueue.delete', tags: ['spacegate'] })
      .input(SpacegateAiQueueRuleInputSchema)
      .output(apiContract.haiResultSchema(SpacegateAiQueueDeleteOutputSchema)),
    resolve: apiContract.route({ method: 'POST', path: '/spacegate/ai-queue/resolve', operationId: 'spacegate.aiQueue.resolve', tags: ['spacegate'] })
      .input(SpacegateAiQueueResolveInputSchema)
      .output(apiContract.haiResultSchema(SpacegateAiQueueResolveOutputSchema)),
  },
}
