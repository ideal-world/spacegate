import type { HaiResult } from '@h-ai/core'
import type {
  PluginInstanceId,
  SpacegateAiQueuePluginInput,
  SpacegateAiQueueReadmeOutput,
  SpacegateAiQueueResolveInput,
  SpacegateAiQueueResolveOutput,
  SpacegateAiQueueRuleInput,
  SpacegateAiQueueRulesInput,
  SpacegateAiQueueRulesOutput,
  SpacegateAiQueueSchemaOutput,
  SpacegateConfigGetOutput,
  SpacegateConfigNameInput,
  SpacegateConfigUpdateInput,
  SpacegateDiscoveryHealthOutput,
  SpacegateDiscoveryReloadGatewayInput,
  SpacegateDiscoveryReloadGlobalInput,
  SpacegateDiscoveryReloadRouteInput,
  SpacegateGatewayDeleteInput,
  SpacegateGatewayGetOutput,
  SpacegateGatewayUpsertInput,
  SpacegateNamesOutput,
  SpacegatePluginAttrOutput,
  SpacegatePluginCodeInput,
  SpacegatePluginDeleteInput,
  SpacegatePluginGetInput,
  SpacegatePluginGetOutput,
  SpacegatePluginListInput,
  SpacegatePluginPreviewWasmSchemaInput,
  SpacegatePluginUpsertInput,
  SpacegatePluginWasmSchemaInput,
  SpacegateResourceListOutput,
  SpacegateRouteDeleteInput,
  SpacegateRouteGetInput,
  SpacegateRouteGetOutput,
  SpacegateRouteListInput,
  SpacegateRouteListOutput,
  SpacegateRouteRenameInput,
  SpacegateRouteUpsertInput,
  SpacegateSchemaOutput,
  SpacegateSchemaPreviewOutput,
  SpacegateVersionOutput,
} from 'spacegate-admin-contract'
import type { z } from 'zod'
import process from 'node:process'
import { core, err, HaiCommonError, ok } from '@h-ai/core'
import {
  SpacegateAiQueueDeleteOutputSchema,
  SpacegateAiQueueReadmeOutputSchema,
  SpacegateAiQueueResolveOutputSchema,
  SpacegateAiQueueRuleOutputSchema,
  SpacegateAiQueueRulesOutputSchema,
  SpacegateAiQueueSchemaOutputSchema,
  SpacegateConfigGetOutputSchema,
  SpacegateDiscoveryHealthOutputSchema,
  SpacegateGatewayGetOutputSchema,
  SpacegateNamesOutputSchema,
  SpacegatePluginAttrOutputSchema,
  SpacegatePluginGetOutputSchema,
  SpacegateResourceListOutputSchema,
  SpacegateRouteGetOutputSchema,
  SpacegateRouteListOutputSchema,
  SpacegateSchemaOutputSchema,
  SpacegateSchemaPreviewOutputSchema,
  SpacegateVersionOutputSchema,
} from 'spacegate-admin-contract'
import { z as zod } from 'zod'

const DEFAULT_ADMIN_BASE_URL = 'http://127.0.0.1:9992'
const DEFAULT_AI_GATEWAY_BASE_URL = 'http://127.0.0.1:18080'
const DEFAULT_REQUEST_TIMEOUT_MS = 10_000
const DEFAULT_TOKEN_TTL_SECONDS = 3_000
const DEFAULT_AI_QUEUE_PLUGIN = 'ai-gateway-queue'

/** 上游配置的 HTTP(S) 基地址；不允许空值或非 HTTP 协议。 */
const BaseUrlSchema = zod.string().trim().min(1).max(2048).refine(
  value => /^https?:\/\//.test(value),
  { message: 'must be an http(s) URL' },
)

/**
 * SpaceGate 上游适配器配置。
 * 凭据只能来自服务端环境变量或 `_spacegate.yml` 的环境引用，不进入前端或日志。
 * @example
 * ```ts
 * const config: SpacegateUpstreamConfig = {
 *   adminBaseUrl: 'http://admin-server:9992',
 *   adminAk: undefined,
 *   adminSk: undefined,
 *   aiGatewayBaseUrl: 'http://ai-gateway:18080',
 *   allowInsecure: true,
 *   requestTimeoutMs: 10_000,
 *   tokenTtlSeconds: 3_000,
 * }
 * ```
 */
export const SpacegateUpstreamConfigSchema = zod.object({
  adminBaseUrl: BaseUrlSchema,
  adminAk: zod.string().min(1).optional(),
  adminSk: zod.string().min(1).optional(),
  aiGatewayBaseUrl: BaseUrlSchema,
  /** 显式允许上游未启用认证；false 时缺少凭据属于配置错误。 */
  allowInsecure: zod.boolean(),
  requestTimeoutMs: zod.number().int().positive(),
  tokenTtlSeconds: zod.number().int().positive(),
})

export type SpacegateUpstreamConfig = z.infer<typeof SpacegateUpstreamConfigSchema>

/** 注入点：测试使用假 fetch，生产默认使用全局 fetch。 */
export type SpacegateFetch = typeof globalThis.fetch

export interface SpacegateUpstreamClientOptions {
  /** 显式配置；省略时在首次请求时从 `core.config` / 环境变量解析。 */
  readonly config?: Partial<SpacegateUpstreamConfig>
  /** 注入的 fetch 实现，便于无网络测试。 */
  readonly fetchFn?: SpacegateFetch
  /** 测试可注入时间源；生产使用 Date.now。 */
  readonly now?: () => number
}

/**
 * SpaceGate 上游客户端。
 *
 * 集中处理 ak/sk 登录、JWT 内存缓存、401 重试一次、版本头、409 冲突和文本错误，
 * procedure 只负责把校验后的输入交给这里。
 */
export interface SpacegateUpstreamClient {
  configNames: () => Promise<HaiResult<SpacegateNamesOutput>>
  configGet: (input: SpacegateConfigNameInput) => Promise<HaiResult<SpacegateConfigGetOutput>>
  configUpdate: (input: SpacegateConfigUpdateInput) => Promise<HaiResult<SpacegateVersionOutput>>
  gatewayGet: (input: SpacegateConfigNameInput) => Promise<HaiResult<SpacegateGatewayGetOutput>>
  gatewayUpsert: (input: SpacegateGatewayUpsertInput) => Promise<HaiResult<SpacegateVersionOutput>>
  gatewayDelete: (input: SpacegateGatewayDeleteInput) => Promise<HaiResult<SpacegateVersionOutput>>
  routeList: (input: SpacegateRouteListInput) => Promise<HaiResult<SpacegateRouteListOutput>>
  routeGet: (input: SpacegateRouteGetInput) => Promise<HaiResult<SpacegateRouteGetOutput>>
  routeUpsert: (input: SpacegateRouteUpsertInput) => Promise<HaiResult<SpacegateVersionOutput>>
  routeRename: (input: SpacegateRouteRenameInput) => Promise<HaiResult<SpacegateVersionOutput>>
  routeDelete: (input: SpacegateRouteDeleteInput) => Promise<HaiResult<SpacegateVersionOutput>>
  pluginList: (input: SpacegatePluginListInput) => Promise<HaiResult<SpacegateResourceListOutput>>
  pluginGet: (input: SpacegatePluginGetInput) => Promise<HaiResult<SpacegatePluginGetOutput>>
  pluginUpsert: (input: SpacegatePluginUpsertInput) => Promise<HaiResult<SpacegateVersionOutput>>
  pluginDelete: (input: SpacegatePluginDeleteInput) => Promise<HaiResult<SpacegateVersionOutput>>
  pluginCatalog: () => Promise<HaiResult<SpacegateNamesOutput>>
  pluginAttrAll: () => Promise<HaiResult<SpacegateResourceListOutput>>
  pluginAttr: (input: SpacegatePluginCodeInput) => Promise<HaiResult<SpacegatePluginAttrOutput>>
  pluginSchema: (input: SpacegatePluginCodeInput) => Promise<HaiResult<SpacegateSchemaOutput>>
  pluginWasmSchema: (input: SpacegatePluginWasmSchemaInput) => Promise<HaiResult<SpacegateSchemaOutput>>
  pluginPreviewWasmSchema: (input: SpacegatePluginPreviewWasmSchemaInput) => Promise<HaiResult<SpacegateSchemaPreviewOutput>>
  pluginRefresh: () => Promise<HaiResult<SpacegateResourceListOutput>>
  discoveryInstances: () => Promise<HaiResult<SpacegateNamesOutput>>
  discoveryHealth: () => Promise<HaiResult<SpacegateDiscoveryHealthOutput>>
  discoveryReloadGateway: (input: SpacegateDiscoveryReloadGatewayInput) => Promise<HaiResult<SpacegateVersionOutput>>
  discoveryReloadGlobal: (input: SpacegateDiscoveryReloadGlobalInput) => Promise<HaiResult<SpacegateVersionOutput>>
  discoveryReloadRoute: (input: SpacegateDiscoveryReloadRouteInput) => Promise<HaiResult<SpacegateVersionOutput>>
  discoveryBackends: () => Promise<HaiResult<SpacegateResourceListOutput>>
  aiQueueSchema: (input: SpacegateAiQueuePluginInput) => Promise<HaiResult<SpacegateAiQueueSchemaOutput>>
  aiQueueReadme: (input: SpacegateAiQueuePluginInput) => Promise<HaiResult<SpacegateAiQueueReadmeOutput>>
  aiQueueRules: (input: SpacegateAiQueueRulesInput) => Promise<HaiResult<SpacegateAiQueueRulesOutput>>
  aiQueueUpsert: (input: SpacegateAiQueueRuleInput) => Promise<HaiResult<{ rule: unknown }>>
  aiQueueDelete: (input: SpacegateAiQueueRuleInput) => Promise<HaiResult<{ deleted: number }>>
  aiQueueResolve: (input: SpacegateAiQueueResolveInput) => Promise<HaiResult<SpacegateAiQueueResolveOutput>>
}

interface RustRequestOptions {
  readonly method: 'GET' | 'POST' | 'PUT' | 'DELETE'
  readonly path: string
  readonly query?: URLSearchParams
  readonly body?: unknown
  /** 写操作由调用方显式提供；GET 省略时使用最近一次已知版本。 */
  readonly version?: number
  /** 写成功后重新读取 `/config/names` 的版本。 */
  readonly refreshVersion?: boolean
}

interface RustResponse {
  readonly body: unknown
  readonly version: number
}

interface AiRequestOptions {
  readonly method: 'GET' | 'PUT' | 'DELETE'
  readonly path: string
  readonly query?: URLSearchParams
  readonly body?: unknown
  /** 读取纯文本（插件 readme）；其他响应按 JSON 解析。 */
  readonly text?: boolean
}

interface CachedToken {
  readonly value: string
  readonly expiresAt: number
}

/** 从 `_spacegate.yml` 或环境变量读取配置；缺失时才使用安全默认值。 */
export function resolveSpacegateUpstreamConfig(overrides?: Partial<SpacegateUpstreamConfig>): SpacegateUpstreamConfig {
  const loaded = core.config.get<Record<string, unknown>>('spacegate') ?? {}
  const raw = {
    adminBaseUrl: pickString(overrides?.adminBaseUrl, loaded.adminBaseUrl, 'HAI_SPACEGATE_ADMINBASEURL', DEFAULT_ADMIN_BASE_URL),
    adminAk: pickOptionalString(overrides?.adminAk, loaded.adminAk, 'HAI_SPACEGATE_ADMINAK'),
    adminSk: pickOptionalString(overrides?.adminSk, loaded.adminSk, 'HAI_SPACEGATE_ADMINSK'),
    aiGatewayBaseUrl: pickString(overrides?.aiGatewayBaseUrl, loaded.aiGatewayBaseUrl, 'HAI_SPACEGATE_AIGATEWAYBASEURL', DEFAULT_AI_GATEWAY_BASE_URL),
    allowInsecure: pickBoolean(overrides?.allowInsecure, loaded.allowInsecure, 'HAI_SPACEGATE_ALLOWINSECURE', false),
    requestTimeoutMs: pickInteger(overrides?.requestTimeoutMs, loaded.requestTimeoutMs, 'HAI_SPACEGATE_REQUESTTIMEOUTMS', DEFAULT_REQUEST_TIMEOUT_MS),
    tokenTtlSeconds: pickInteger(overrides?.tokenTtlSeconds, loaded.tokenTtlSeconds, 'HAI_SPACEGATE_TOKENTTLSECONDS', DEFAULT_TOKEN_TTL_SECONDS),
  }
  const parsed = SpacegateUpstreamConfigSchema.safeParse(raw)
  if (!parsed.success) {
    // 只输出字段路径和约束说明，避免把凭据值写进错误信息或日志。
    const detail = parsed.error.issues.map(issue => `${issue.path.join('.') || 'config'}: ${issue.message}`).join('; ')
    throw new Error(`SpaceGate upstream configuration error: ${detail}`)
  }
  if (Boolean(parsed.data.adminAk) !== Boolean(parsed.data.adminSk))
    throw new Error('SpaceGate upstream configuration error: adminAk and adminSk must be configured together')
  if (!parsed.data.allowInsecure && (!parsed.data.adminAk || !parsed.data.adminSk))
    throw new Error('SpaceGate upstream configuration error: adminAk/adminSk are required unless allowInsecure is true')
  return parsed.data
}

/**
 * 创建 SpaceGate 上游客户端；配置在首次请求时校验，避免仅创建 App 就要求上游凭据。
 * @param options - 显式配置、注入 fetch 和时间源（测试用）
 * @returns 线程内共享 JWT 缓存的客户端实例
 * @example
 * ```ts
 * const client = createSpacegateUpstreamClient()
 * const names = await client.configNames()
 * if (names.success) console.info(names.data.items, names.data.version)
 * ```
 */
export function createSpacegateUpstreamClient(options: SpacegateUpstreamClientOptions = {}): SpacegateUpstreamClient {
  return new SpacegateUpstreamClientImpl(options)
}

class SpacegateUpstreamClientImpl implements SpacegateUpstreamClient {
  private readonly fetchFn: SpacegateFetch
  private readonly now: () => number
  private readonly overrides: Partial<SpacegateUpstreamConfig> | undefined
  private resolvedConfig: SpacegateUpstreamConfig | undefined
  private token: CachedToken | undefined
  private loginInFlight: Promise<HaiResult<string>> | undefined
  private lastVersion: number | undefined

  constructor(options: SpacegateUpstreamClientOptions) {
    this.fetchFn = options.fetchFn ?? globalThis.fetch
    this.now = options.now ?? Date.now
    this.overrides = options.config
  }

  async configNames(): Promise<HaiResult<SpacegateNamesOutput>> {
    const response = await this.request({ method: 'GET', path: '/config/names' })
    return parseVersioned(response, SpacegateNamesOutputSchema, (body, version) => ({ items: asStringArray(body), version }))
  }

  async configGet(input: SpacegateConfigNameInput): Promise<HaiResult<SpacegateConfigGetOutput>> {
    const response = await this.request({ method: 'GET', path: `/config/item/${encodeSegment(input.name)}` })
    return parseVersioned(response, SpacegateConfigGetOutputSchema, (body, version) => ({ item: body ?? null, version }))
  }

  async configUpdate(input: SpacegateConfigUpdateInput): Promise<HaiResult<SpacegateVersionOutput>> {
    const response = await this.request({ method: 'PUT', path: '/config', body: input.item, version: input.version, refreshVersion: true })
    return parseVersioned(response, SpacegateVersionOutputSchema, (_, version) => ({ version }))
  }

  async gatewayGet(input: SpacegateConfigNameInput): Promise<HaiResult<SpacegateGatewayGetOutput>> {
    const response = await this.request({ method: 'GET', path: `/config/item/${encodeSegment(input.name)}/gateway` })
    return parseVersioned(response, SpacegateGatewayGetOutputSchema, (body, version) => ({ gateway: body ?? null, version }))
  }

  async gatewayUpsert(input: SpacegateGatewayUpsertInput): Promise<HaiResult<SpacegateVersionOutput>> {
    const response = await this.request({
      method: input.create ? 'POST' : 'PUT',
      path: `/config/item/${encodeSegment(input.name)}/gateway`,
      body: input.gateway,
      version: input.version,
      refreshVersion: true,
    })
    return parseVersioned(response, SpacegateVersionOutputSchema, (_, version) => ({ version }))
  }

  async gatewayDelete(input: SpacegateGatewayDeleteInput): Promise<HaiResult<SpacegateVersionOutput>> {
    const response = await this.request({
      method: 'DELETE',
      path: `/config/item/${encodeSegment(input.name)}/gateway`,
      version: input.version,
      refreshVersion: true,
    })
    return parseVersioned(response, SpacegateVersionOutputSchema, (_, version) => ({ version }))
  }

  async routeList(input: SpacegateRouteListInput): Promise<HaiResult<SpacegateRouteListOutput>> {
    const response = await this.request({ method: 'GET', path: `/config/item/${encodeSegment(input.gateway)}/route/all` })
    return parseVersioned(response, SpacegateRouteListOutputSchema, (body, version) => {
      const routes = isRecord(body) ? body : {}
      return { items: Object.entries(routes).map(([name, route]) => ({ name, route })), version }
    })
  }

  async routeGet(input: SpacegateRouteGetInput): Promise<HaiResult<SpacegateRouteGetOutput>> {
    const response = await this.request({ method: 'GET', path: routePath(input.gateway, input.route) })
    return parseVersioned(response, SpacegateRouteGetOutputSchema, (body, version) => ({ route: body ?? null, version }))
  }

  async routeUpsert(input: SpacegateRouteUpsertInput): Promise<HaiResult<SpacegateVersionOutput>> {
    const response = await this.request({
      method: input.create ? 'POST' : 'PUT',
      path: routePath(input.gateway, input.route),
      body: input.config,
      version: input.version,
      refreshVersion: true,
    })
    return parseVersioned(response, SpacegateVersionOutputSchema, (_, version) => ({ version }))
  }

  async routeRename(input: SpacegateRouteRenameInput): Promise<HaiResult<SpacegateVersionOutput>> {
    const body = { ...input.config, route_name: input.newName }
    const response = await this.request({
      method: 'POST',
      path: `${routePath(input.gateway, input.route)}/rename`,
      body,
      version: input.version,
      refreshVersion: true,
    })
    return parseVersioned(response, SpacegateVersionOutputSchema, (_, version) => ({ version }))
  }

  async routeDelete(input: SpacegateRouteDeleteInput): Promise<HaiResult<SpacegateVersionOutput>> {
    const response = await this.request({
      method: 'DELETE',
      path: routePath(input.gateway, input.route),
      version: input.version,
      refreshVersion: true,
    })
    return parseVersioned(response, SpacegateVersionOutputSchema, (_, version) => ({ version }))
  }

  async pluginList(input: SpacegatePluginListInput): Promise<HaiResult<SpacegateResourceListOutput>> {
    // 旧管理端的插件中心/选择器按 code 查询；省略 code 时保留全量列表语义。
    const path = input.code ? `/config/plugins/${encodeSegment(input.code)}` : '/config/plugin-all'
    const response = await this.request({ method: 'GET', path })
    return parseVersioned(response, SpacegateResourceListOutputSchema, (body, version) => ({ items: asArray(body), version }))
  }

  async pluginGet(input: SpacegatePluginGetInput): Promise<HaiResult<SpacegatePluginGetOutput>> {
    const response = await this.request({ method: 'GET', path: '/config/plugin', query: pluginIdQuery(input.id) })
    return parseVersioned(response, SpacegatePluginGetOutputSchema, (body, version) => ({ plugin: body ?? null, version }))
  }

  async pluginUpsert(input: SpacegatePluginUpsertInput): Promise<HaiResult<SpacegateVersionOutput>> {
    const query = pluginIdQuery(input.plugin.id)
    const displayName = input.plugin.displayName?.trim()
    if (displayName)
      query.set('display_name', displayName)
    const response = await this.request({
      method: input.create ? 'POST' : 'PUT',
      path: '/config/plugin',
      query,
      body: input.plugin.spec,
      version: input.version,
      refreshVersion: true,
    })
    return parseVersioned(response, SpacegateVersionOutputSchema, (_, version) => ({ version }))
  }

  async pluginDelete(input: SpacegatePluginDeleteInput): Promise<HaiResult<SpacegateVersionOutput>> {
    const response = await this.request({
      method: 'DELETE',
      path: '/config/plugin',
      query: pluginIdQuery(input.id),
      version: input.version,
      refreshVersion: true,
    })
    return parseVersioned(response, SpacegateVersionOutputSchema, (_, version) => ({ version }))
  }

  async pluginCatalog(): Promise<HaiResult<SpacegateNamesOutput>> {
    const response = await this.request({ method: 'GET', path: '/plugin/list' })
    return parseVersioned(response, SpacegateNamesOutputSchema, (body, version) => ({ items: asStringArray(body), version }))
  }

  async pluginAttrAll(): Promise<HaiResult<SpacegateResourceListOutput>> {
    const response = await this.request({ method: 'GET', path: '/plugin/attr-all' })
    return parseVersioned(response, SpacegateResourceListOutputSchema, (body, version) => ({ items: asArray(body), version }))
  }

  async pluginAttr(input: SpacegatePluginCodeInput): Promise<HaiResult<SpacegatePluginAttrOutput>> {
    const response = await this.request({ method: 'GET', path: `/plugin/attr/${encodeSegment(input.code)}` })
    return parseVersioned(response, SpacegatePluginAttrOutputSchema, (body, version) => ({ attr: body ?? null, version }))
  }

  async pluginSchema(input: SpacegatePluginCodeInput): Promise<HaiResult<SpacegateSchemaOutput>> {
    const response = await this.request({ method: 'GET', path: `/plugin/schema/${encodeSegment(input.code)}` })
    return parseVersioned(response, SpacegateSchemaOutputSchema, (body, version) => ({ schema: body, version }))
  }

  async pluginWasmSchema(input: SpacegatePluginWasmSchemaInput): Promise<HaiResult<SpacegateSchemaOutput>> {
    const response = await this.request({ method: 'GET', path: '/plugin/wasm/schema', query: pluginIdQuery(input.id) })
    return parseVersioned(response, SpacegateSchemaOutputSchema, (body, version) => ({ schema: body, version }))
  }

  async pluginPreviewWasmSchema(input: SpacegatePluginPreviewWasmSchemaInput): Promise<HaiResult<SpacegateSchemaPreviewOutput>> {
    // Rust 只读预览接口不参与版本控制，也不返回 X-Server-Version。
    const response = await this.request({
      method: 'POST',
      path: '/plugin/wasm/schema/preview',
      body: {
        image_url: input.imageUrl,
        ...(input.schemaPath ? { schema_path: input.schemaPath } : {}),
        ...(input.ociAuth === undefined ? {} : { oci_auth: input.ociAuth }),
        ...(input.httpHeaders === undefined ? {} : { http_headers: input.httpHeaders }),
      },
    })
    return parseVersioned(response, SpacegateSchemaPreviewOutputSchema, body => ({ schema: body }))
  }

  async pluginRefresh(): Promise<HaiResult<SpacegateResourceListOutput>> {
    // 上游将其视为只读 POST：不需要客户端版本，也不会在响应头返回版本。
    const response = await this.request({ method: 'POST', path: '/plugin/refresh' })
    return parseVersioned(response, SpacegateResourceListOutputSchema, (body, version) => ({ items: asArray(body), version }))
  }

  async discoveryInstances(): Promise<HaiResult<SpacegateNamesOutput>> {
    const response = await this.request({ method: 'GET', path: '/discovery/instance/list' })
    return parseVersioned(response, SpacegateNamesOutputSchema, (body, version) => ({ items: asStringArray(body), version }))
  }

  async discoveryHealth(): Promise<HaiResult<SpacegateDiscoveryHealthOutput>> {
    const response = await this.request({ method: 'GET', path: '/discovery/instance/health' })
    return parseVersioned(response, SpacegateDiscoveryHealthOutputSchema, (body, version) => {
      const health: Record<string, boolean> = {}
      if (isRecord(body)) {
        for (const [key, value] of Object.entries(body))
          health[key] = value === true
      }
      return { health, version }
    })
  }

  async discoveryReloadGateway(input: SpacegateDiscoveryReloadGatewayInput): Promise<HaiResult<SpacegateVersionOutput>> {
    const query = new URLSearchParams({ instance: input.instance, gateway: input.gateway })
    const response = await this.request({ method: 'POST', path: '/discovery/instance/reload/gateway', query, version: input.version, refreshVersion: true })
    return parseVersioned(response, SpacegateVersionOutputSchema, (_, version) => ({ version }))
  }

  async discoveryReloadGlobal(input: SpacegateDiscoveryReloadGlobalInput): Promise<HaiResult<SpacegateVersionOutput>> {
    const query = new URLSearchParams({ instance: input.instance })
    const response = await this.request({ method: 'POST', path: '/discovery/instance/reload/global', query, version: input.version, refreshVersion: true })
    return parseVersioned(response, SpacegateVersionOutputSchema, (_, version) => ({ version }))
  }

  async discoveryReloadRoute(input: SpacegateDiscoveryReloadRouteInput): Promise<HaiResult<SpacegateVersionOutput>> {
    const query = new URLSearchParams({ instance: input.instance, gateway: input.gateway, route: input.route })
    const response = await this.request({ method: 'POST', path: '/discovery/instance/reload/route', query, version: input.version, refreshVersion: true })
    return parseVersioned(response, SpacegateVersionOutputSchema, (_, version) => ({ version }))
  }

  async discoveryBackends(): Promise<HaiResult<SpacegateResourceListOutput>> {
    const response = await this.request({ method: 'GET', path: '/discovery/backends' })
    return parseVersioned(response, SpacegateResourceListOutputSchema, (body, version) => ({ items: asArray(body), version }))
  }

  async aiQueueSchema(input: SpacegateAiQueuePluginInput): Promise<HaiResult<SpacegateAiQueueSchemaOutput>> {
    const response = await this.aiRequest({ method: 'GET', path: aiQueuePluginPath(input, 'schema') })
    return parseAi(response, SpacegateAiQueueSchemaOutputSchema, body => ({ schema: body }))
  }

  async aiQueueReadme(input: SpacegateAiQueuePluginInput): Promise<HaiResult<SpacegateAiQueueReadmeOutput>> {
    const response = await this.aiRequest({ method: 'GET', path: aiQueuePluginPath(input, 'readme'), text: true })
    return parseAi(response, SpacegateAiQueueReadmeOutputSchema, body => ({ markdown: typeof body === 'string' ? body : '' }))
  }

  async aiQueueRules(input: SpacegateAiQueueRulesInput): Promise<HaiResult<SpacegateAiQueueRulesOutput>> {
    const response = await this.aiRequest({ method: 'GET', path: '/v1/admin/tenant-rate-limits', query: filterQuery(input.filters) })
    return parseAi(response, SpacegateAiQueueRulesOutputSchema, body => ({ items: asArray(body) }))
  }

  async aiQueueUpsert(input: SpacegateAiQueueRuleInput): Promise<HaiResult<{ rule: unknown }>> {
    const response = await this.aiRequest({ method: 'PUT', path: '/v1/admin/tenant-rate-limits', body: input.rule })
    return parseAi(response, SpacegateAiQueueRuleOutputSchema, body => ({ rule: body }))
  }

  async aiQueueDelete(input: SpacegateAiQueueRuleInput): Promise<HaiResult<{ deleted: number }>> {
    const response = await this.aiRequest({ method: 'DELETE', path: '/v1/admin/tenant-rate-limits', body: input.rule })
    return parseAi(response, SpacegateAiQueueDeleteOutputSchema, body => ({
      deleted: isRecord(body) && typeof body.deleted === 'number' ? body.deleted : 0,
    }))
  }

  async aiQueueResolve(input: SpacegateAiQueueResolveInput): Promise<HaiResult<SpacegateAiQueueResolveOutput>> {
    const query = new URLSearchParams({ tenant: input.tenant })
    for (const key of ['model', 'path', 'policy'] as const) {
      const value = input[key]
      if (value)
        query.set(key, value)
    }
    const response = await this.aiRequest({ method: 'GET', path: '/v1/admin/tenant-rate-limits/resolve', query })
    return parseAi(response, SpacegateAiQueueResolveOutputSchema, body => ({ resolution: body }))
  }

  /** 解析并缓存配置；首次请求才校验，凭据缺失时明确失败而不是静默降级。 */
  private getConfig(): SpacegateUpstreamConfig {
    if (!this.resolvedConfig)
      this.resolvedConfig = resolveSpacegateUpstreamConfig(this.overrides)
    return this.resolvedConfig
  }

  private authEnabled(config: SpacegateUpstreamConfig): boolean {
    return Boolean(config.adminAk && config.adminSk)
  }

  /** 执行 Rust admin-server 请求；401 只重登录并重试一次，写操作冲突永不重试。 */
  private async request(options: RustRequestOptions): Promise<HaiResult<RustResponse>> {
    const config = this.getConfig()
    const response = await this.sendRust(config, options, false)
    if (!response.success)
      return response
    const result = response.data
    const text = await result.text()
    const serverVersion = parseVersionHeader(result.headers.get('x-server-version'))
    if (!result.ok)
      return upstreamError(result.status, text, serverVersion)
    if (serverVersion !== undefined)
      this.lastVersion = serverVersion
    let version = serverVersion ?? 0
    if (options.refreshVersion) {
      const refreshed = await this.readVersion(config)
      if (!refreshed.success)
        return refreshed
      version = refreshed.data
    }
    return ok({ body: parseBody(text), version })
  }

  private async sendRust(config: SpacegateUpstreamConfig, options: RustRequestOptions, retried: boolean): Promise<HaiResult<Response>> {
    const headers = new Headers({ accept: 'application/json' })
    if (options.body !== undefined)
      headers.set('content-type', 'application/json')

    if (this.authEnabled(config)) {
      const token = await this.getToken(config)
      if (!token.success)
        return token
      if (token.data)
        headers.set('authorization', `Bearer ${token.data}`)
    }

    const clientVersion = options.version ?? (options.method === 'GET' ? this.lastVersion ?? 0 : undefined)
    if (clientVersion !== undefined)
      headers.set('x-client-version', String(clientVersion))

    let response: Response
    try {
      response = await this.fetchFn(buildUrl(config.adminBaseUrl, options.path, options.query), {
        method: options.method,
        headers,
        ...(options.body === undefined ? {} : { body: JSON.stringify(options.body) }),
        signal: AbortSignal.timeout(config.requestTimeoutMs),
      })
    }
    catch (error) {
      return transportError(error, options.method, options.path)
    }

    // 只记录方法/路径/状态，绝不输出 ak、sk、JWT 或请求体。
    core.logger.debug('SpaceGate upstream request', { method: options.method, path: options.path, status: response.status })

    if (response.status === 401 && this.authEnabled(config) && !retried) {
      this.token = undefined
      return this.sendRust(config, options, true)
    }
    return ok(response)
  }

  /** 写成功后按 Rust 协议重新读取配置版本；上游写响应本身不返回新版本。 */
  private async readVersion(config: SpacegateUpstreamConfig): Promise<HaiResult<number>> {
    const result = await this.sendRust(config, { method: 'GET', path: '/config/names' }, false)
    if (!result.success)
      return result
    const response = result.data
    await response.text()
    const version = parseVersionHeader(response.headers.get('x-server-version'))
    if (version !== undefined)
      this.lastVersion = version
    return ok(version ?? 0)
  }

  /** 获取或刷新上游 JWT；缓存在内存，401 时由调用方清除。 */
  private async getToken(config: SpacegateUpstreamConfig): Promise<HaiResult<string>> {
    if (!config.adminAk || !config.adminSk)
      return ok('')
    if (this.token && this.token.expiresAt > this.now())
      return ok(this.token.value)
    if (this.loginInFlight)
      return this.loginInFlight
    const promise = this.login(config).finally(() => {
      if (this.loginInFlight === promise)
        this.loginInFlight = undefined
    })
    this.loginInFlight = promise
    return promise
  }

  private async login(config: SpacegateUpstreamConfig): Promise<HaiResult<string>> {
    let response: Response
    try {
      response = await this.fetchFn(buildUrl(config.adminBaseUrl, '/auth/login'), {
        method: 'POST',
        headers: new Headers({ 'content-type': 'application/json', 'accept': 'application/json' }),
        body: JSON.stringify({ ak: config.adminAk, sk: config.adminSk }),
        signal: AbortSignal.timeout(config.requestTimeoutMs),
      })
    }
    catch (error) {
      return transportError(error, 'POST', '/auth/login')
    }
    core.logger.debug('SpaceGate upstream request', { method: 'POST', path: '/auth/login', status: response.status })
    const text = await response.text()
    if (!response.ok)
      return upstreamError(response.status, text)
    const jwt = readJwtCookie(response.headers)
    // 配置了凭据却拿不到 JWT 通常意味着上游认证被关闭；不能静默降级为匿名访问。
    if (!jwt)
      return err(HaiCommonError.UNAUTHORIZED, 'SpaceGate upstream login succeeded without a JWT cookie; configure allowInsecure only for an explicitly unauthenticated development upstream')
    this.token = { value: jwt, expiresAt: this.now() + config.tokenTtlSeconds * 1000 }
    return ok(jwt)
  }

  private async aiRequest(options: AiRequestOptions): Promise<HaiResult<unknown>> {
    const config = this.getConfig()
    const headers = new Headers({ accept: options.text ? 'text/markdown' : 'application/json' })
    if (options.body !== undefined)
      headers.set('content-type', 'application/json')
    let response: Response
    try {
      response = await this.fetchFn(buildUrl(config.aiGatewayBaseUrl, options.path, options.query), {
        method: options.method,
        headers,
        ...(options.body === undefined ? {} : { body: JSON.stringify(options.body) }),
        signal: AbortSignal.timeout(config.requestTimeoutMs),
      })
    }
    catch (error) {
      return transportError(error, options.method, options.path)
    }
    core.logger.debug('AI Gateway request', { method: options.method, path: options.path, status: response.status })
    const text = await response.text()
    if (!response.ok)
      return upstreamError(response.status, text)
    return ok(options.text ? text : parseBody(text))
  }
}

/** 校验版本化响应；解析失败说明上游协议变化，返回 API_ERROR 而不是静默丢字段。 */
function parseVersioned<T>(
  response: HaiResult<RustResponse>,
  schema: z.ZodType<T>,
  build: (body: unknown, version: number) => T,
): HaiResult<T> {
  if (!response.success)
    return response
  const parsed = schema.safeParse(build(response.data.body, response.data.version))
  if (!parsed.success)
    return invalidResponse(parsed.error)
  return ok(parsed.data)
}

/** plugin / aiQueue 等无版本响应；`build` 只负责把上游 body 映射到 contract 形状。 */
function parseAi<T>(response: HaiResult<unknown>, schema: z.ZodType<T>, build: (body: unknown) => T): HaiResult<T> {
  if (!response.success)
    return response
  const parsed = schema.safeParse(build(response.data))
  if (!parsed.success)
    return invalidResponse(parsed.error)
  return ok(parsed.data)
}

/** 把上游错误映射为 contract 错误码；文本 body 原样保留，便于管理界面展示。 */
function upstreamError(status: number, text: string, serverVersion?: number): HaiResult<never> {
  const message = text.trim() || `SpaceGate upstream returned HTTP ${status}`
  if (status === 401)
    return err(HaiCommonError.UNAUTHORIZED, message)
  if (status === 404)
    return err(HaiCommonError.NOT_FOUND, message)
  if (status === 409) {
    const versionHint = serverVersion === undefined ? '' : `serverVersion=${serverVersion}`
    return err(HaiCommonError.CONFLICT, versionHint ? `${versionHint}: ${message}` : message)
  }
  if (status >= 500)
    return err(status === 502 || status === 503 || status === 504 ? HaiCommonError.SERVICE_UNAVAILABLE : HaiCommonError.API_ERROR, message)
  return err(HaiCommonError.API_ERROR, message)
}

/** 网络/超时错误保持可区分，避免把上游不可达误报成业务失败。 */
function transportError(error: unknown, method: string, path: string): HaiResult<never> {
  if (error instanceof Error && (error.name === 'AbortError' || error.name === 'TimeoutError'))
    return err(HaiCommonError.TIMEOUT, `SpaceGate upstream request timed out: ${method} ${path}`)
  return err(HaiCommonError.NETWORK_ERROR, error instanceof Error ? error.message : `SpaceGate upstream request failed: ${method} ${path}`)
}

function invalidResponse(error: z.ZodError): HaiResult<never> {
  const issue = error.issues[0]
  const detail = issue ? `${issue.path.join('.') || 'response'}: ${issue.message}` : 'invalid payload'
  return err(HaiCommonError.API_ERROR, `Unexpected SpaceGate upstream response: ${detail}`)
}

/** 构造带查询参数的完整上游 URL；base 的尾部斜杠会被去掉。 */
function buildUrl(base: string, path: string, query?: URLSearchParams): string {
  const url = new URL(`${base.replace(/\/+$/, '')}${path}`)
  if (query) {
    for (const [key, value] of query)
      url.searchParams.append(key, value)
  }
  return url.toString()
}

/** 路径片段编码；名称校验已禁止 `/`，这里仍编码以防其他特殊字符。 */
function encodeSegment(value: string): string {
  return encodeURIComponent(value)
}

function routePath(gateway: string, route: string): string {
  return `/config/item/${encodeSegment(gateway)}/route/item/${encodeSegment(route)}`
}

/** 插件实例查询参数：named 用 name，anon 用 uid。 */
function pluginIdQuery(id: PluginInstanceId): URLSearchParams {
  const query = new URLSearchParams({ code: id.code, kind: id.kind })
  if (id.kind === 'named')
    query.set('name', id.name)
  else
    query.set('uid', id.uid)
  return query
}

/** AI Gateway 插件路径；省略插件名时使用 ai-gateway-queue。 */
function aiQueuePluginPath(input: SpacegateAiQueuePluginInput, action: 'schema' | 'readme'): string {
  return `/v1/admin/plugins/${encodeSegment(input.plugin ?? DEFAULT_AI_QUEUE_PLUGIN)}/${action}`
}

/** 过滤条件只发送非空值，保持与现有管理前端一致。 */
function filterQuery(filters: Record<string, unknown> | undefined): URLSearchParams {
  const query = new URLSearchParams()
  if (!filters)
    return query
  for (const [key, value] of Object.entries(filters)) {
    if (value === undefined || value === null)
      continue
    const text = String(value).trim()
    if (text)
      query.set(key, text)
  }
  return query
}

/** 从 Set-Cookie 提取 jwt；不记录 cookie 内容。 */
function readJwtCookie(headers: Headers): string | undefined {
  const values = typeof headers.getSetCookie === 'function'
    ? headers.getSetCookie()
    : [headers.get('set-cookie') ?? '']
  for (const value of values) {
    for (const part of value.split(',')) {
      const pair = part.trim().split(';')[0] ?? ''
      if (pair.startsWith('jwt='))
        return pair.slice(4)
    }
  }
  return undefined
}

function parseVersionHeader(value: string | null): number | undefined {
  if (value === null || value.trim() === '')
    return undefined
  const parsed = Number(value)
  return Number.isInteger(parsed) && parsed >= 0 ? parsed : undefined
}

function parseBody(text: string): unknown {
  if (text === '')
    return undefined
  try {
    return JSON.parse(text)
  }
  catch {
    return text
  }
}

function asArray(value: unknown): unknown[] {
  return Array.isArray(value) ? value : []
}

function asStringArray(value: unknown): string[] {
  return Array.isArray(value) ? value.filter((item): item is string => typeof item === 'string') : []
}

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === 'object' && value !== null && !Array.isArray(value)
}

function pickString(override: string | undefined, loaded: unknown, envKey: string, fallback: string): string {
  if (override !== undefined)
    return override
  if (typeof loaded === 'string' && loaded.length > 0)
    return loaded
  const envValue = process.env[envKey]
  return envValue !== undefined && envValue !== '' ? envValue : fallback
}

function pickOptionalString(override: string | undefined, loaded: unknown, envKey: string): string | undefined {
  if (override !== undefined)
    return override
  if (typeof loaded === 'string' && loaded.length > 0)
    return loaded
  const envValue = process.env[envKey]
  return envValue !== undefined && envValue !== '' ? envValue : undefined
}

function pickBoolean(override: boolean | undefined, loaded: unknown, envKey: string, fallback: boolean): boolean {
  if (override !== undefined)
    return override
  if (typeof loaded === 'boolean')
    return loaded
  const envValue = process.env[envKey]
  if (envValue === undefined || envValue === '')
    return fallback
  return ['1', 'true', 'yes', 'on'].includes(envValue.toLowerCase())
}

function pickInteger(override: number | undefined, loaded: unknown, envKey: string, fallback: number): number {
  if (override !== undefined)
    return override
  if (typeof loaded === 'number' && Number.isFinite(loaded))
    return Math.trunc(loaded)
  const envValue = Number(process.env[envKey])
  return Number.isFinite(envValue) && envValue > 0 ? Math.trunc(envValue) : fallback
}
