import { z } from 'zod'

/**
 * SpaceGate 资源名（网关、路由、实例）：非空、不含 `/`，并限制长度。
 * 名称会作为 Rust admin-server 的路径片段，服务端和上游都会再次校验。
 * @example
 * ```ts
 * SpacegateNameSchema.parse('prod-gateway')
 * ```
 */
export const SpacegateNameSchema = z.string().trim().min(1).max(128).refine(value => !value.includes('/'), {
  message: 'name must not contain "/"',
})

/** 插件代码：字母或数字开头，仅允许字母数字与 `.`、`_`、`-`。 */
export const PluginCodeSchema = z.string().trim().min(1).max(64).regex(/^[a-z0-9][\w.-]*$/i, {
  message: 'plugin code must match ^[a-zA-Z0-9][a-zA-Z0-9._-]*$',
})

/** AI Gateway 插件名只允许小写字母、数字和短横线。 */
export const AiQueuePluginNameSchema = z.string().trim().min(1).max(64).regex(/^[a-z0-9][a-z0-9-]*$/, {
  message: 'ai queue plugin name must match ^[a-z0-9][a-z0-9-]*$',
})

/**
 * 配置版本号：对应 Rust `X-Server-Version`，必须是非负整数。
 * 所有写 procedure 都要求调用方回传自己读取到的版本，服务端不得替换为全局最新值。
 */
export const SpacegateVersionSchema = z.number().int().min(0)

/** 插件实例 ID；`named` 使用网关内唯一名称，`anon` 使用 uid。 */
export const PluginInstanceIdSchema = z.discriminatedUnion('kind', [
  z.object({
    code: PluginCodeSchema,
    kind: z.literal('named'),
    name: SpacegateNameSchema,
  }).strict(),
  z.object({
    code: PluginCodeSchema,
    kind: z.literal('anon'),
    uid: z.string().trim().min(1).max(128).refine(value => !value.includes('/'), {
      message: 'uid must not contain "/"',
    }),
  }).strict(),
])

/**
 * 可 JSON 序列化的值；插件 `spec` 必须存在，避免向上游发送空 body。
 * 嵌套结构保持宽松，未知字段原样往返。
 */
export const SpacegateJsonValueSchema = z.union([
  z.null(),
  z.boolean(),
  z.number(),
  z.string(),
  z.array(z.unknown()),
  z.record(z.string(), z.unknown()),
])

/**
 * 插件管理配置：`id` 决定实例身份，`displayName` 只用于展示，`spec` 为原样保存的运行时 JSON。
 * 使用 looseObject 保留 Rust 侧未来新增字段，避免读取后再写入时丢字段。
 */
export const PluginConfigSchema = z.looseObject({
  id: PluginInstanceIdSchema,
  displayName: z.string().trim().max(128).optional(),
  spec: SpacegateJsonValueSchema,
})

/**
 * Rust 资源 JSON（Config、ConfigItem、SgGateway、SgRoute、限流规则等）。
 * 不逐字段建模，保留未知字段往返；仅保证是 JSON object。
 */
export const SpacegateResourceSchema = z.looseObject({})

/** 无输入 procedure 的空对象输入；拒绝额外字段以尽早暴露调用方错误。 */
export const SpacegateEmptyInputSchema = z.object({}).strict()

/** config.names / config.get 共用的名称输入。 */
export const SpacegateConfigNameInputSchema = z.object({
  name: SpacegateNameSchema,
}).strict()

/** config.update：完整配置文档 + 调用方读取到的版本。 */
export const SpacegateConfigUpdateInputSchema = z.object({
  item: SpacegateResourceSchema,
  version: SpacegateVersionSchema,
}).strict()

/** gateway.get：网关名称。 */
export const SpacegateGatewayGetInputSchema = SpacegateConfigNameInputSchema

/** gateway.upsert：`create` 选择上游 POST（创建）或 PUT（更新）。 */
export const SpacegateGatewayUpsertInputSchema = z.object({
  name: SpacegateNameSchema,
  gateway: SpacegateResourceSchema,
  version: SpacegateVersionSchema,
  create: z.boolean(),
}).strict()

/** gateway.delete：删除指定网关。 */
export const SpacegateGatewayDeleteInputSchema = z.object({
  name: SpacegateNameSchema,
  version: SpacegateVersionSchema,
}).strict()

/** route.list：列出网关下所有路由。 */
export const SpacegateRouteListInputSchema = z.object({
  gateway: SpacegateNameSchema,
}).strict()

/** route.get：读取单个路由完整配置。 */
export const SpacegateRouteGetInputSchema = z.object({
  gateway: SpacegateNameSchema,
  route: SpacegateNameSchema,
}).strict()

/** route.upsert：创建或更新路由。 */
export const SpacegateRouteUpsertInputSchema = z.object({
  gateway: SpacegateNameSchema,
  route: SpacegateNameSchema,
  config: SpacegateResourceSchema,
  version: SpacegateVersionSchema,
  create: z.boolean(),
}).strict()

/** route.rename：重命名时提交完整路由配置并覆盖 `route_name`。 */
export const SpacegateRouteRenameInputSchema = z.object({
  gateway: SpacegateNameSchema,
  route: SpacegateNameSchema,
  newName: SpacegateNameSchema,
  config: SpacegateResourceSchema,
  version: SpacegateVersionSchema,
}).strict()

/** route.delete：删除指定路由。 */
export const SpacegateRouteDeleteInputSchema = z.object({
  gateway: SpacegateNameSchema,
  route: SpacegateNameSchema,
  version: SpacegateVersionSchema,
}).strict()

/** plugin.get / plugin.delete：插件实例 ID。 */
export const SpacegatePluginGetInputSchema = z.object({
  id: PluginInstanceIdSchema,
}).strict()

/** plugin.list：可选按插件 code 过滤；省略时返回全部实例。 */
export const SpacegatePluginListInputSchema = z.object({
  code: PluginCodeSchema.optional(),
}).strict()

/** plugin.upsert：完整插件配置 + 版本 + 创建/更新模式。 */
export const SpacegatePluginUpsertInputSchema = z.object({
  plugin: PluginConfigSchema,
  version: SpacegateVersionSchema,
  create: z.boolean(),
}).strict()

/** plugin.delete：插件实例 ID + 版本。 */
export const SpacegatePluginDeleteInputSchema = z.object({
  id: PluginInstanceIdSchema,
  version: SpacegateVersionSchema,
}).strict()

/** plugin.attr / plugin.schema：插件代码。 */
export const SpacegatePluginCodeInputSchema = z.object({
  code: PluginCodeSchema,
}).strict()

/** plugin.wasmSchema：按实例 ID 查找已保存的 Wasm schema。 */
export const SpacegatePluginWasmSchemaInputSchema = z.object({
  id: PluginInstanceIdSchema,
}).strict()

/**
 * plugin.previewWasmSchema：预览镜像内 schema。
 * `imageUrl` 使用普通字符串校验以兼容 `oci+http(s)://` 前缀，不能只接受 http(s) URL。
 */
export const SpacegatePluginPreviewWasmSchemaInputSchema = z.object({
  imageUrl: z.string().trim().min(1).max(2048),
  schemaPath: z.string().trim().min(1).max(512).optional(),
  ociAuth: z.unknown().optional(),
  /** 私有 HTTP(S) Schema 下载的认证/签名头；只允许字符串值。 */
  httpHeaders: z.record(z.string(), z.string()).optional(),
}).strict()

/** discovery.reloadGateway：实例 + 网关 + 版本。 */
export const SpacegateDiscoveryReloadGatewayInputSchema = z.object({
  instance: SpacegateNameSchema,
  gateway: SpacegateNameSchema,
  version: SpacegateVersionSchema,
}).strict()

/** discovery.reloadGlobal：实例 + 版本。 */
export const SpacegateDiscoveryReloadGlobalInputSchema = z.object({
  instance: SpacegateNameSchema,
  version: SpacegateVersionSchema,
}).strict()

/** discovery.reloadRoute：实例 + 网关 + 路由 + 版本。 */
export const SpacegateDiscoveryReloadRouteInputSchema = z.object({
  instance: SpacegateNameSchema,
  gateway: SpacegateNameSchema,
  route: SpacegateNameSchema,
  version: SpacegateVersionSchema,
}).strict()

/** AI Gateway 插件查询：省略时使用 `ai-gateway-queue`。 */
export const SpacegateAiQueuePluginInputSchema = z.object({
  plugin: AiQueuePluginNameSchema.optional(),
}).strict()

/** AI 队列限流规则过滤条件；空字符串由适配器忽略。 */
export const SpacegateAiQueueFiltersSchema = z.looseObject({
  tenant: z.string().trim().max(128).optional(),
  model: z.string().trim().max(128).optional(),
  path: z.string().trim().max(512).optional(),
  policy: z.enum(['abandon', 'queue', 'wait']).optional(),
})

/** AI 队列限流规则列表查询。 */
export const SpacegateAiQueueRulesInputSchema = z.object({
  filters: SpacegateAiQueueFiltersSchema.optional(),
}).strict()

/** AI 队列规则新增/更新/删除；规则结构由 AI Gateway 维护，原样透传。 */
export const SpacegateAiQueueRuleInputSchema = z.object({
  rule: SpacegateResourceSchema,
}).strict()

/** AI 队列规则解析查询。 */
export const SpacegateAiQueueResolveInputSchema = z.object({
  tenant: z.string().trim().min(1).max(128),
  model: z.string().trim().max(128).optional(),
  path: z.string().trim().max(512).optional(),
  policy: z.enum(['abandon', 'queue', 'wait']).optional(),
}).strict()

/** config.names / plugin.catalog / discovery.instances 共用的字符串列表 + 版本输出。 */
export const SpacegateNamesOutputSchema = z.object({
  items: z.array(z.string()),
  version: SpacegateVersionSchema,
})

/** config.get / gateway.get / route.get / plugin.get 共用的可空资源 + 版本输出。 */
export const SpacegateConfigGetOutputSchema = z.object({
  item: z.unknown().nullable(),
  version: SpacegateVersionSchema,
})

/** 所有写操作成功后的新版本。 */
export const SpacegateVersionOutputSchema = z.object({
  version: SpacegateVersionSchema,
})

/** gateway.get 输出。 */
export const SpacegateGatewayGetOutputSchema = z.object({
  gateway: z.unknown().nullable(),
  version: SpacegateVersionSchema,
})

/** route.list 的单项：路由名与其完整配置。 */
export const SpacegateRouteItemSchema = z.object({
  name: z.string(),
  route: z.unknown(),
})

/** route.list 输出。 */
export const SpacegateRouteListOutputSchema = z.object({
  items: z.array(SpacegateRouteItemSchema),
  version: SpacegateVersionSchema,
})

/** route.get 输出。 */
export const SpacegateRouteGetOutputSchema = z.object({
  route: z.unknown().nullable(),
  version: SpacegateVersionSchema,
})

/** plugin.list / plugin.attrAll / discovery.backends / aiQueue.rules 共用的资源列表输出 + 版本。 */
export const SpacegateResourceListOutputSchema = z.object({
  items: z.array(z.unknown()),
  version: SpacegateVersionSchema,
})

/** plugin.get 输出。 */
export const SpacegatePluginGetOutputSchema = z.object({
  plugin: z.unknown().nullable(),
  version: SpacegateVersionSchema,
})

/** plugin.attr 输出。 */
export const SpacegatePluginAttrOutputSchema = z.object({
  attr: z.unknown().nullable(),
  version: SpacegateVersionSchema,
})

/** plugin.schema / plugin.wasmSchema 输出。 */
export const SpacegateSchemaOutputSchema = z.object({
  schema: z.unknown(),
  version: SpacegateVersionSchema,
})

/** plugin.previewWasmSchema 输出；只读预览不参与版本控制。 */
export const SpacegateSchemaPreviewOutputSchema = z.object({
  schema: z.unknown(),
})

/** discovery.health 输出。 */
export const SpacegateDiscoveryHealthOutputSchema = z.object({
  health: z.record(z.string(), z.boolean()),
  version: SpacegateVersionSchema,
})

/** aiQueue.schema 输出。 */
export const SpacegateAiQueueSchemaOutputSchema = z.object({
  schema: z.unknown(),
})

/** aiQueue.readme 输出。 */
export const SpacegateAiQueueReadmeOutputSchema = z.object({
  markdown: z.string(),
})

/** aiQueue.rules 输出。 */
export const SpacegateAiQueueRulesOutputSchema = z.object({
  items: z.array(z.unknown()),
})

/** aiQueue.upsert 输出。 */
export const SpacegateAiQueueRuleOutputSchema = z.object({
  rule: z.unknown(),
})

/** aiQueue.delete 输出。 */
export const SpacegateAiQueueDeleteOutputSchema = z.object({
  deleted: z.number().int(),
})

/** aiQueue.resolve 输出。 */
export const SpacegateAiQueueResolveOutputSchema = z.object({
  resolution: z.unknown(),
})

export type SpacegateName = z.infer<typeof SpacegateNameSchema>
export type PluginCode = z.infer<typeof PluginCodeSchema>
export type SpacegateVersion = z.infer<typeof SpacegateVersionSchema>
export type PluginInstanceId = z.infer<typeof PluginInstanceIdSchema>
export type PluginConfig = z.infer<typeof PluginConfigSchema>
export type SpacegateConfigNameInput = z.infer<typeof SpacegateConfigNameInputSchema>
export type SpacegateConfigUpdateInput = z.infer<typeof SpacegateConfigUpdateInputSchema>
export type SpacegateGatewayUpsertInput = z.infer<typeof SpacegateGatewayUpsertInputSchema>
export type SpacegateGatewayDeleteInput = z.infer<typeof SpacegateGatewayDeleteInputSchema>
export type SpacegateRouteListInput = z.infer<typeof SpacegateRouteListInputSchema>
export type SpacegateRouteGetInput = z.infer<typeof SpacegateRouteGetInputSchema>
export type SpacegateRouteUpsertInput = z.infer<typeof SpacegateRouteUpsertInputSchema>
export type SpacegateRouteRenameInput = z.infer<typeof SpacegateRouteRenameInputSchema>
export type SpacegateRouteDeleteInput = z.infer<typeof SpacegateRouteDeleteInputSchema>
export type SpacegatePluginGetInput = z.infer<typeof SpacegatePluginGetInputSchema>
export type SpacegatePluginListInput = z.infer<typeof SpacegatePluginListInputSchema>
export type SpacegatePluginUpsertInput = z.infer<typeof SpacegatePluginUpsertInputSchema>
export type SpacegatePluginDeleteInput = z.infer<typeof SpacegatePluginDeleteInputSchema>
export type SpacegatePluginCodeInput = z.infer<typeof SpacegatePluginCodeInputSchema>
export type SpacegatePluginWasmSchemaInput = z.infer<typeof SpacegatePluginWasmSchemaInputSchema>
export type SpacegatePluginPreviewWasmSchemaInput = z.infer<typeof SpacegatePluginPreviewWasmSchemaInputSchema>
export type SpacegateDiscoveryReloadGatewayInput = z.infer<typeof SpacegateDiscoveryReloadGatewayInputSchema>
export type SpacegateDiscoveryReloadGlobalInput = z.infer<typeof SpacegateDiscoveryReloadGlobalInputSchema>
export type SpacegateDiscoveryReloadRouteInput = z.infer<typeof SpacegateDiscoveryReloadRouteInputSchema>
export type SpacegateAiQueuePluginInput = z.infer<typeof SpacegateAiQueuePluginInputSchema>
export type SpacegateAiQueueFilters = z.infer<typeof SpacegateAiQueueFiltersSchema>
export type SpacegateAiQueueRulesInput = z.infer<typeof SpacegateAiQueueRulesInputSchema>
export type SpacegateAiQueueRuleInput = z.infer<typeof SpacegateAiQueueRuleInputSchema>
export type SpacegateAiQueueResolveInput = z.infer<typeof SpacegateAiQueueResolveInputSchema>
export type SpacegateNamesOutput = z.infer<typeof SpacegateNamesOutputSchema>
export type SpacegateConfigGetOutput = z.infer<typeof SpacegateConfigGetOutputSchema>
export type SpacegateVersionOutput = z.infer<typeof SpacegateVersionOutputSchema>
export type SpacegateGatewayGetOutput = z.infer<typeof SpacegateGatewayGetOutputSchema>
export type SpacegateRouteItem = z.infer<typeof SpacegateRouteItemSchema>
export type SpacegateRouteListOutput = z.infer<typeof SpacegateRouteListOutputSchema>
export type SpacegateRouteGetOutput = z.infer<typeof SpacegateRouteGetOutputSchema>
export type SpacegateResourceListOutput = z.infer<typeof SpacegateResourceListOutputSchema>
export type SpacegatePluginGetOutput = z.infer<typeof SpacegatePluginGetOutputSchema>
export type SpacegatePluginAttrOutput = z.infer<typeof SpacegatePluginAttrOutputSchema>
export type SpacegateSchemaOutput = z.infer<typeof SpacegateSchemaOutputSchema>
export type SpacegateSchemaPreviewOutput = z.infer<typeof SpacegateSchemaPreviewOutputSchema>
export type SpacegateDiscoveryHealthOutput = z.infer<typeof SpacegateDiscoveryHealthOutputSchema>
export type SpacegateAiQueueSchemaOutput = z.infer<typeof SpacegateAiQueueSchemaOutputSchema>
export type SpacegateAiQueueReadmeOutput = z.infer<typeof SpacegateAiQueueReadmeOutputSchema>
export type SpacegateAiQueueRulesOutput = z.infer<typeof SpacegateAiQueueRulesOutputSchema>
export type SpacegateAiQueueRuleOutput = z.infer<typeof SpacegateAiQueueRuleOutputSchema>
export type SpacegateAiQueueDeleteOutput = z.infer<typeof SpacegateAiQueueDeleteOutputSchema>
export type SpacegateAiQueueResolveOutput = z.infer<typeof SpacegateAiQueueResolveOutputSchema>
