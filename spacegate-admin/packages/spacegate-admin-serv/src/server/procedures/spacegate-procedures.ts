import type { ServContext } from '@h-ai/serv'
import type { SpacegateUpstreamClient } from '../upstream/spacegate-upstream-client.js'
import { serv } from '@h-ai/serv'
import { spacegateContract } from 'spacegate-admin-contract'

/**
 * SpaceGate 管理 procedures。
 *
 * 所有读写路由都要求 `admin` 角色；普通用户只能访问个人资料/会话接口。
 * handler 保持薄层，把校验后的 camelCase 输入交给上游适配器，并原样返回 HaiResult。
 * @param client - 进程内共享的上游客户端（JWT 缓存和版本快照随之复用）
 * @returns 可直接挂载到工程级 procedures 的 router
 * @example
 * ```ts
 * const procedures = createSpacegateProcedures(createSpacegateUpstreamClient())
 * ```
 */
export function createSpacegateProcedures(client: SpacegateUpstreamClient) {
  return serv.implement(spacegateContract).context<ServContext>().route('config.names').role('admin').handle(() => client.configNames()).route('config.get').role('admin').handle(({ input }) => client.configGet(input)).route('config.update').role('admin').handle(({ input }) => client.configUpdate(input)).route('gateway.get').role('admin').handle(({ input }) => client.gatewayGet(input)).route('gateway.upsert').role('admin').handle(({ input }) => client.gatewayUpsert(input)).route('gateway.delete').role('admin').handle(({ input }) => client.gatewayDelete(input)).route('route.list').role('admin').handle(({ input }) => client.routeList(input)).route('route.get').role('admin').handle(({ input }) => client.routeGet(input)).route('route.upsert').role('admin').handle(({ input }) => client.routeUpsert(input)).route('route.rename').role('admin').handle(({ input }) => client.routeRename(input)).route('route.delete').role('admin').handle(({ input }) => client.routeDelete(input)).route('plugin.list').role('admin').handle(({ input }) => client.pluginList(input)).route('plugin.get').role('admin').handle(({ input }) => client.pluginGet(input)).route('plugin.upsert').role('admin').handle(({ input }) => client.pluginUpsert(input)).route('plugin.delete').role('admin').handle(({ input }) => client.pluginDelete(input)).route('plugin.catalog').role('admin').handle(() => client.pluginCatalog()).route('plugin.attrAll').role('admin').handle(() => client.pluginAttrAll()).route('plugin.attr').role('admin').handle(({ input }) => client.pluginAttr(input)).route('plugin.schema').role('admin').handle(({ input }) => client.pluginSchema(input)).route('plugin.wasmSchema').role('admin').handle(({ input }) => client.pluginWasmSchema(input)).route('plugin.previewWasmSchema').role('admin').handle(({ input }) => client.pluginPreviewWasmSchema(input)).route('plugin.refresh').role('admin').handle(() => client.pluginRefresh()).route('discovery.instances').role('admin').handle(() => client.discoveryInstances()).route('discovery.health').role('admin').handle(() => client.discoveryHealth()).route('discovery.reloadGateway').role('admin').handle(({ input }) => client.discoveryReloadGateway(input)).route('discovery.reloadGlobal').role('admin').handle(({ input }) => client.discoveryReloadGlobal(input)).route('discovery.reloadRoute').role('admin').handle(({ input }) => client.discoveryReloadRoute(input)).route('discovery.backends').role('admin').handle(() => client.discoveryBackends()).route('aiQueue.schema').role('admin').handle(({ input }) => client.aiQueueSchema(input)).route('aiQueue.readme').role('admin').handle(({ input }) => client.aiQueueReadme(input)).route('aiQueue.rules').role('admin').handle(({ input }) => client.aiQueueRules(input)).route('aiQueue.upsert').role('admin').handle(({ input }) => client.aiQueueUpsert(input)).route('aiQueue.delete').role('admin').handle(({ input }) => client.aiQueueDelete(input)).route('aiQueue.resolve').role('admin').handle(({ input }) => client.aiQueueResolve(input)).build()
}
