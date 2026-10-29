import type { PluginConfig, PluginInstanceId } from 'spacegate-admin-contract'

/** SpaceGate 上游资源在共享层保持宽松 JSON，未知字段读取后原样写回。 */
export type JsonRecord = Record<string, unknown>

/**
 * JSON 对象窄化；数组、null 和标量都返回 null。
 * @example asRecord({ name: 'demo' })
 */
export function asRecord(value: unknown): JsonRecord | null {
  return typeof value === 'object' && value !== null && !Array.isArray(value) ? value as JsonRecord : null
}

/**
 * 从字段读取对象数组，非对象元素被忽略但长度变化不影响调用方按索引编辑的语义。
 * 仅用于渲染与逐项编辑，真正写回时仍以原始数组为基准保留未知元素。
 * @example asRecordArray([{ name: 'a' }, null])
 */
export function asRecordArray(value: unknown): JsonRecord[] {
  return Array.isArray(value) ? value.filter((item): item is JsonRecord => asRecord(item) !== null) : []
}

/**
 * 深拷贝纯 JSON 值，避免编辑器直接污染 store 中的对象。
 * 只处理 JSON 可表示的值；函数、Date 等运行时对象不会出现在配置里。
 * @example cloneValue({ listeners: [] })
 */
export function cloneValue<T>(value: T): T {
  if (Array.isArray(value))
    return value.map(item => cloneValue(item)) as T
  const record = asRecord(value)
  if (record) {
    const result: JsonRecord = {}
    for (const [key, item] of Object.entries(record))
      result[key] = cloneValue(item)
    return result as T
  }
  return value
}

/** 克隆 JSON 对象；非对象返回空对象。 */
export function cloneRecord(value: unknown): JsonRecord {
  return cloneValue(asRecord(value) ?? {})
}

/** 读取字符串字段；非字符串返回空字符串。 */
export function readString(source: JsonRecord | null | undefined, key: string): string {
  const value = source?.[key]
  return typeof value === 'string' ? value : ''
}

/** 读取数字字段；非有限数字返回 undefined。 */
export function readNumber(source: JsonRecord | null | undefined, key: string): number | undefined {
  const value = source?.[key]
  return typeof value === 'number' && Number.isFinite(value) ? value : undefined
}

/** 读取布尔字段；非布尔返回 undefined。 */
export function readBoolean(source: JsonRecord | null | undefined, key: string): boolean | undefined {
  const value = source?.[key]
  return typeof value === 'boolean' ? value : undefined
}

/** 读取对象字段；缺失或非对象返回 undefined。 */
export function readRecord(source: JsonRecord | null | undefined, key: string): JsonRecord | undefined {
  return asRecord(source?.[key]) ?? undefined
}

/** 网关名称；兼容配置项和网关对象两种输入。 */
export function gatewayNameOf(value: unknown): string {
  return readString(asRecord(value), 'name')
}

/** 路由名称；Rust 路由对象使用 `route_name`。 */
export function routeNameOf(value: unknown): string {
  return readString(asRecord(value), 'route_name')
}

/** MCP 路由通过 `kind: 'MCPRoute'` 标记；HTTP 路由没有 kind 字段。 */
export function isMcpRoute(route: unknown): boolean {
  return asRecord(route)?.kind === 'MCPRoute'
}

/** 路由类型标签值，仅用于 UI 选择。 */
export type RouteType = 'http' | 'mcp'

/** 从路由 JSON 判断类型。 */
export function routeTypeOf(route: unknown): RouteType {
  return isMcpRoute(route) ? 'mcp' : 'http'
}

/** 当前路由的首条匹配或 MCP 传输摘要。 */
export function routeMatchSummary(route: unknown): string {
  const record = asRecord(route)
  if (!record)
    return ''
  if (isMcpRoute(record)) {
    if (record.transport === 'legacy_sse') {
      const legacy = readRecord(record, 'legacy_sse')
      return `${readString(legacy, 'sse_path')} + ${readString(legacy, 'message_path')}`
    }
    return readString(record, 'path')
  }
  const rules = asRecordArray(record.rules)
  const firstMatch = asRecordArray(rules[0]?.matches).at(0) ?? asRecord(rules[0]?.matches)
  const path = readRecord(firstMatch, 'path')
  const pathValue = readString(path, 'value')
  const summary = pathValue ? `${readString(path, 'kind') || 'Prefix'} ${pathValue}` : ''
  return rules.length > 1 ? `${summary || '*'} +${rules.length - 1}` : summary
}

/** 首个后端摘要；用于路由和网关列表。 */
export function backendSummary(backend: unknown): string {
  const record = asRecord(backend)
  const host = readRecord(record, 'host')
  if (!host)
    return ''
  const kind = readString(host, 'kind')
  if (kind === 'K8sService')
    return `${readString(host, 'name')}.${readString(host, 'namespace')}`
  if (kind === 'File')
    return readString(host, 'path')
  return readString(host, 'host')
}

/**
 * 插件实例引用；上游插件配置同时存在 named、anon、mono 三种形态。
 * Contract 只允许 named/anon 写操作，mono 在 UI 中只读展示。
 */
export type PluginReference = { code: string } & (
  | { kind: 'named', name: string }
  | { kind: 'anon', uid: string }
  | { kind: 'mono' }
)

/** UI 使用的插件实例视图，保留原始 JSON 以便写回未知字段。 */
export interface PluginInstance {
  ref: PluginReference
  displayName: string
  spec: JsonRecord
  raw: JsonRecord
}

/** 从任意上游 JSON 读取插件引用。 */
export function pluginReferenceOf(value: unknown): PluginReference | null {
  const record = asRecord(value)
  const code = readString(record, 'code')
  const kind = readString(record, 'kind')
  if (!code)
    return null
  if (kind === 'named') {
    const name = readString(record, 'name')
    return name ? { code, kind, name } : null
  }
  if (kind === 'anon') {
    const uid = readString(record, 'uid')
    return uid ? { code, kind, uid } : null
  }
  if (kind === 'mono')
    return { code, kind }
  return null
}

/** 插件引用转换为 contract 写入 ID；mono 不支持管理端写入。 */
export function pluginIdOf(value: unknown): PluginInstanceId | null {
  const ref = pluginReferenceOf(value)
  if (!ref || ref.kind === 'mono')
    return null
  return ref
}

/** 插件实例的稳定前端 key：`code-n-name` / `code-a-uid` / `code-m`。 */
export function pluginKey(ref: PluginReference): string {
  if (ref.kind === 'named')
    return `${ref.code}-n-${ref.name}`
  if (ref.kind === 'anon')
    return `${ref.code}-a-${ref.uid}`
  return `${ref.code}-m`
}

/**
 * 判断插件代码是否属于自定义 Wasm 扩展：`wasm`、`wasm.*` 或 `wasm-*`。
 * 原生插件（如 hai-auth、redis-limit）与内置 AI 队列插件都不满足该规则。
 * @example isWasmPluginCode('wasm-header') // true
 */
export function isWasmPluginCode(code: string): boolean {
  const normalized = code.trim().toLowerCase()
  return normalized === 'wasm' || normalized.startsWith('wasm.') || normalized.startsWith('wasm-')
}

/**
 * 插件中心维护的自定义 Wasm 定义：Wasm 代码 + 具名实例 + 未绑定到资源，
 * 并排除内置的 AI 网关队列插件；绑定到网关/路由的 Wasm 配置不在插件中心显示。
 * @example isWasmPluginDefinition({ ref: { code: 'wasm', kind: 'named', name: 'x' }, spec: {}, ... }) // true
 */
export function isWasmPluginDefinition(instance: PluginInstance): boolean {
  if (instance.ref.kind !== 'named' || !isWasmPluginCode(instance.ref.code))
    return false
  if (instance.spec.binding_scope !== undefined)
    return false
  return instance.spec.plugin_name !== 'ai-gateway-queue'
}

/** 实例名称，绑定编辑器用于辨识。 */
export function pluginReferenceName(ref: PluginReference): string {
  if (ref.kind === 'named')
    return ref.name
  if (ref.kind === 'anon')
    return ref.uid
  return ref.code
}

/** 从上游插件配置 JSON 构造 UI 视图。 */
export function pluginInstanceOf(value: unknown): PluginInstance | null {
  const raw = asRecord(value)
  const ref = pluginReferenceOf(raw)
  if (!raw || !ref)
    return null
  return {
    ref,
    displayName: readString(raw, 'display_name') || readString(raw, 'displayName'),
    spec: cloneRecord(raw.spec),
    raw,
  }
}

/** 校验插件引用可写后再转换为 contract PluginConfig。 */
export function pluginConfigOf(instance: PluginInstance, spec: JsonRecord, displayName: string): PluginConfig | null {
  const id = pluginIdOf(instance.ref)
  if (!id)
    return null
  return {
    id,
    displayName: displayName.trim() || undefined,
    spec: cloneRecord(spec),
  }
}

/** 绑定中的插件引用；绑定与实例 ID 共用 code/kind/name/uid 字段。 */
export function bindingReferenceOf(value: unknown): PluginReference | null {
  return pluginReferenceOf(value)
}

/** 绑定的整数优先级；缺失时按 0 处理。 */
export function bindingPriority(value: unknown): number {
  return readNumber(asRecord(value), 'priority') ?? 0
}

/** 插件选择器的返回结果：引用已有实例，或按插件代码新建实例。 */
export interface PluginPick {
  /** `instance` 表示选中已有实例；`plugin` 表示要新建该插件的实例。 */
  type: 'instance' | 'plugin'
  /** 选中已有实例时的实例本身。 */
  instance?: PluginInstance
  /** 新建实例时使用的插件代码。 */
  code?: string
}

/** 构造绑定项，保留引用结构并显式写入优先级。 */
export function createBinding(ref: PluginReference, priority = 0): JsonRecord {
  return { ...ref, priority }
}

/** 网关默认模型；字段与 Rust `SgGateway` 保持一致。 */
export function newGateway(name: string): JsonRecord {
  return {
    name,
    parameters: {
      redis_url: null,
      redis_pool_max_size: null,
      redis_pool_wait_timeout_ms: null,
      redis_pool_create_timeout_ms: null,
      redis_pool_recycle_timeout_ms: null,
      log_level: null,
      lang: null,
      ignore_tls_verification: null,
      enable_x_request_id: false,
      observability: {
        enabled: false,
        service_name: 'spacegate',
        otlp_endpoint: 'http://localhost:4317',
        protocol: 'grpc',
        traces: { enabled: false, sample_ratio: 1 },
        metrics: { enabled: false, export_interval_ms: 60000 },
        logs: { enabled: false, level: 'info' },
      },
    },
    listeners: [{
      name: 'http',
      ip: null,
      port: 9000,
      protocol: { type: 'http' },
      hostname: null,
    }],
    plugins: [],
  }
}

/** 新建监听器默认值。 */
export function newListener(index: number): JsonRecord {
  return { name: `listener-${index}`, ip: null, port: 8080, protocol: { type: 'http' }, hostname: null }
}

/** 新建 HTTP 后端默认值。 */
export function newBackend(): JsonRecord {
  return {
    host: { kind: 'Host', host: '127.0.0.1' },
    port: 80,
    timeout_ms: null,
    timeout_mode: null,
    protocol: 'http',
    downgrade_http2: false,
    weight: 1,
    plugins: [],
  }
}

/** 新建 HTTP 规则默认值：Prefix `/` 匹配到本机 80 端口。 */
export function newHttpRule(): JsonRecord {
  return {
    matches: [{ path: { kind: 'Prefix', value: '/', replace: null }, header: null, query: null, method: null }],
    plugins: [],
    backends: [newBackend()],
    timeout_ms: null,
    timeout_mode: null,
    balance_policy: null,
  }
}

/** 新建 HTTP 路由默认模型。 */
export function newHttpRoute(name: string): JsonRecord {
  return {
    route_name: name,
    hostnames: null,
    priority: 0,
    plugins: [],
    rules: [newHttpRule()],
  }
}

/** 新建 MCP 路由默认模型。 */
export function newMcpRoute(name: string): JsonRecord {
  return {
    kind: 'MCPRoute',
    route_name: name,
    hostnames: null,
    transport: 'streamable_http',
    path: '/mcp',
    legacy_sse: null,
    backends: [newBackend()],
    plugins: [],
    timeout_mode: 'disabled',
    session_affinity: 'mcp_session',
  }
}

/** 根据路由类型构造编辑草稿。 */
export function newRoute(name: string, type: RouteType): JsonRecord {
  return type === 'mcp' ? newMcpRoute(name) : newHttpRoute(name)
}

/** 从上游错误消息读取 `serverVersion=N`。 */
export function parseServerVersion(message: string): number | null {
  const match = /serverVersion=(\d+)/.exec(message)
  return match ? Number(match[1]) : null
}

/** 稳定格式化 JSON，便于导入/导出与只读展示。 */
export function formatJson(value: unknown): string {
  try {
    return JSON.stringify(value, null, 2)
  }
  catch {
    return ''
  }
}

/** 解析 JSON；失败返回错误，不抛出。 */
export function parseJson(value: string): { ok: true, value: unknown } | { ok: false } {
  try {
    return { ok: true, value: JSON.parse(value) }
  }
  catch {
    return { ok: false }
  }
}
