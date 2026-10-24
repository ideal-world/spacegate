import type {
  AdminUser,
  PluginInstanceId,
  ProfileUpdate,
  SpacegateAiQueueDeleteOutput,
  SpacegateAiQueueReadmeOutput,
  SpacegateAiQueueResolveInput,
  SpacegateAiQueueResolveOutput,
  SpacegateAiQueueRuleInput,
  SpacegateAiQueueRulesInput,
  SpacegateAiQueueRulesOutput,
  SpacegateAiQueueSchemaOutput,
  SpacegateConfigGetOutput,
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
  SpacegatePluginDeleteInput,
  SpacegatePluginGetOutput,
  SpacegatePluginListInput,
  SpacegatePluginPreviewWasmSchemaInput,
  SpacegatePluginUpsertInput,
  SpacegateResourceListOutput,
  SpacegateRouteDeleteInput,
  SpacegateRouteGetOutput,
  SpacegateRouteListInput,
  SpacegateRouteListOutput,
  SpacegateRouteRenameInput,
  SpacegateRouteUpsertInput,
  SpacegateSchemaOutput,
  SpacegateSchemaPreviewOutput,
  SpacegateVersionOutput,
} from 'spacegate-admin-contract'

/**
 * 小程序固定客户端标识；服务端据此开放 body refresh token 通道。
 * 必须与 `spacegate-admin-serv/src/app-server.ts` 的 `MINIAPP_REQUEST_CLIENT` 保持一致。
 */
export const API_CLIENT_NAME = 'spacegate-admin-miniapp'
/** 本地开发默认地址；真机/生产必须改为 HTTPS 反向代理地址。 */
export const DEFAULT_API_BASE_URL = 'http://127.0.0.1:3001/api/miniapp/v1'
/** 与微信原生请求共用；所有请求都必须声明该头。 */
const TOKEN_TRANSPORT = 'body'
const REQUEST_TIMEOUT_MS = 15_000

/** 认证返回的用户摘要。 */
export interface MiniUser {
  id: string
  username: string
  displayName?: string
  email?: string
  phone?: string
  enabled?: boolean
  metadata?: Record<string, unknown>
}

/** 当前用户（含角色与权限）。 */
export interface MiniCurrentUser extends MiniUser {
  roles: string[]
  permissions: string[]
}

/** IAM token 对；refreshToken 只在内存保存。 */
export interface MiniTokenPair {
  accessToken: string
  refreshToken?: string
  expiresIn?: number
  tokenType?: 'Bearer'
}

/** 登录成功数据。 */
export interface MiniLoginResult {
  user: MiniUser
  tokens: MiniTokenPair
  roles: string[]
  permissions: string[]
}

/** 公开认证配置，用于决定是否显示 LDAP 入口。 */
export interface MiniPublicConfig {
  login: { password: boolean, ldap: boolean, otp?: boolean, email?: boolean, sms?: boolean }
  register?: { enabled: boolean }
}

/** 归一化后的服务端错误体。 */
interface ApiErrorBody {
  code: string | number
  message: string
  httpStatus?: number
  ext?: Record<string, unknown>
}

type Envelope<T> = { success: true, data: T } | { success: false, error: ApiErrorBody }

/** 通用 API 错误；页面按类型映射本地化文案。 */
export class ApiError extends Error {
  readonly code: string | number
  readonly httpStatus?: number

  constructor(message: string, code: string | number = 'API_ERROR', httpStatus?: number) {
    super(message)
    this.name = 'ApiError'
    this.code = code
    this.httpStatus = httpStatus
  }
}

/** 网络不可达、超时或 `wx.request` 失败。 */
export class NetworkError extends ApiError {
  constructor(message = 'Network request failed') {
    super(message, 'NETWORK', 0)
    this.name = 'NetworkError'
  }
}

/** 401：需要重新登录。 */
export class UnauthorizedError extends ApiError {
  constructor(message = 'Unauthorized', code: string | number = 'UNAUTHORIZED') {
    super(message, code, 401)
    this.name = 'UnauthorizedError'
  }
}

/** access/refresh token 均已失效，内存会话已清理。 */
export class SessionExpiredError extends UnauthorizedError {
  constructor(message = 'Session expired') {
    super(message, 'SESSION_EXPIRED')
    this.name = 'SessionExpiredError'
  }
}

/** 403：当前角色无权执行。 */
export class ForbiddenError extends ApiError {
  constructor(message = 'Forbidden') {
    super(message, 'FORBIDDEN', 403)
    this.name = 'ForbiddenError'
  }
}

/** 400/422：输入校验失败。 */
export class ValidationError extends ApiError {
  constructor(message = 'Validation failed', code: string | number = 'VALIDATION_ERROR') {
    super(message, code, 400)
    this.name = 'ValidationError'
  }
}

/**
 * 409 配置版本冲突。
 *
 * `serverVersion` 来自服务端错误信息中的 `serverVersion=`（或 `error.ext`），
 * 页面必须先重新读取资源再让用户确认，不能自动重试写请求。
 */
export class ConflictError extends ApiError {
  readonly serverVersion?: number

  constructor(message = 'Version conflict', serverVersion?: number) {
    super(message, 'CONFLICT', 409)
    this.name = 'ConflictError'
    this.serverVersion = serverVersion
  }
}

/** 服务端响应不是合法的 HaiResult 结构。 */
export class InvalidResponseError extends ApiError {
  constructor(message = 'Invalid response payload', httpStatus?: number) {
    super(message, 'INVALID_RESPONSE', httpStatus)
    this.name = 'InvalidResponseError'
  }
}

interface RequestOptions {
  method?: 'GET' | 'POST'
  auth?: boolean
  retryOnUnauthorized?: boolean
}

let apiBaseUrl = ''
let accessToken: string | undefined
let refreshToken: string | undefined
let cachedUser: MiniCurrentUser | undefined
let refreshTask: Promise<void> | undefined
/** 会话代次：clearSession 后让仍在途的刷新结果失效，避免登出后被重新写入 token。 */
let sessionEpoch = 0

/** 判断普通对象，用于校验服务端 envelope 与 JSON 文本。 */
export function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === 'object' && value !== null && !Array.isArray(value)
}

function wxApi(): WxApi {
  if (typeof wx === 'undefined')
    throw new NetworkError('WeChat runtime is unavailable')
  return wx
}

function readExtBaseUrl(): string | undefined {
  const api = wxApi()
  if (typeof api.getExtConfigSync !== 'function')
    return undefined
  let ext: Record<string, unknown>
  try {
    ext = api.getExtConfigSync()
  }
  catch {
    return undefined
  }
  for (const key of ['apiBaseUrl', 'apiBase', 'HAI_MINIAPP_API_BASE_URL']) {
    const value = ext[key]
    if (typeof value === 'string' && /^https?:\/\//.test(value.trim()))
      return value.trim().replace(/\/+$/, '')
  }
  return undefined
}

/**
 * 设置 API 基地址；必须以 http(s) 开头，末尾斜杠会被去掉。
 * @param value - 反向代理后的 `/api/miniapp/v1` 基地址
 * @throws 协议不是 http(s) 时抛出
 * @example
 * ```ts
 * configureApi({ baseUrl: 'https://admin.example.com/api/miniapp/v1' })
 * ```
 */
export function configureApi(options: { baseUrl?: string }): void {
  if (options.baseUrl === undefined)
    return
  const value = options.baseUrl.trim().replace(/\/+$/, '')
  if (!/^https?:\/\//.test(value))
    throw new ApiError('API base URL must use http(s)', 'INVALID_BASE_URL')
  apiBaseUrl = value
}

/**
 * 解析当前请求基地址：显式配置 -> `wx.getExtConfigSync()` -> 本地默认值。
 * 微信小程序无法访问构建环境变量，部署时通过 ext 配置或 `configureApi` 注入。
 */
export function getApiBaseUrl(): string {
  return apiBaseUrl || readExtBaseUrl() || DEFAULT_API_BASE_URL
}

/** 是否持有内存 access token（不代表服务端仍接受）。 */
export function hasSession(): boolean {
  return Boolean(accessToken)
}

/** 当前内存中的用户摘要。 */
export function getCachedUser(): MiniCurrentUser | undefined {
  return cachedUser
}

/** 清理内存 token 与用户；不会写入 storage 或日志。 */
export function clearSession(): void {
  sessionEpoch++
  accessToken = undefined
  refreshToken = undefined
  cachedUser = undefined
}

function applyTokens(tokens: MiniTokenPair): void {
  if (!tokens.accessToken)
    throw new InvalidResponseError('Login response is missing accessToken')
  accessToken = tokens.accessToken
  if (tokens.refreshToken)
    refreshToken = tokens.refreshToken
}

/** 校验并归一化 HaiResult；不合法时返回 undefined 交由调用方抛错。 */
function readEnvelope<T>(payload: unknown): Envelope<T> | undefined {
  if (!isRecord(payload) || typeof payload.success !== 'boolean')
    return undefined
  if (payload.success) {
    if (!('data' in payload))
      return undefined
    return { success: true, data: payload.data as T }
  }
  if (!isRecord(payload.error))
    return undefined
  const raw = payload.error
  const code = typeof raw.code === 'string' || typeof raw.code === 'number' ? raw.code : 'API_ERROR'
  const message = typeof raw.message === 'string' ? raw.message : 'Request failed'
  const httpStatus = typeof raw.httpStatus === 'number' ? raw.httpStatus : undefined
  const ext = isRecord(raw.ext) ? raw.ext : undefined
  return { success: false, error: { code, message, ...(httpStatus === undefined ? {} : { httpStatus }), ...(ext ? { ext } : {}) } }
}

function parseServerVersion(error: ApiErrorBody): number | undefined {
  const match = /serverVersion=(\d+)/.exec(error.message)
  if (match?.[1])
    return Number(match[1])
  for (const key of ['serverVersion', 'version']) {
    const value = error.ext?.[key]
    if (typeof value === 'number' && Number.isInteger(value) && value >= 0)
      return value
    if (typeof value === 'string' && /^\d+$/.test(value))
      return Number(value)
  }
  return undefined
}

function toApiError(error: ApiErrorBody, fallbackStatus?: number): ApiError {
  const status = error.httpStatus ?? fallbackStatus
  if (status === 401)
    return new UnauthorizedError(error.message, error.code)
  if (status === 403)
    return new ForbiddenError(error.message)
  if (status === 409)
    return new ConflictError(error.message, parseServerVersion(error))
  if (status === 400 || status === 422)
    return new ValidationError(error.message, error.code)
  return new ApiError(error.message, error.code, status)
}

function send<T>(path: string, input: unknown, method: 'GET' | 'POST', auth: boolean): Promise<Envelope<T>> {
  return new Promise<Envelope<T>>((resolve, reject) => {
    const header: Record<string, string> = {
      'content-type': 'application/json',
      'x-request-client': API_CLIENT_NAME,
      'x-hai-token-transport': TOKEN_TRANSPORT,
    }
    if (auth && accessToken)
      header.authorization = `Bearer ${accessToken}`
    try {
      wxApi().request<T>({
        url: `${getApiBaseUrl()}${path}`,
        method,
        data: input,
        header,
        timeout: REQUEST_TIMEOUT_MS,
        dataType: 'json',
        success: (result) => {
          const envelope = readEnvelope<T>(result.data)
          if (!envelope) {
            reject(new InvalidResponseError(`Unexpected response for ${path}`, result.statusCode))
            return
          }
          if (!envelope.success && envelope.error.httpStatus === undefined && result.statusCode >= 400)
            envelope.error.httpStatus = result.statusCode
          resolve(envelope)
        },
        fail: (error) => {
          reject(new NetworkError(error.errMsg || `Request failed for ${path}`))
        },
      })
    }
    catch (error) {
      reject(error instanceof Error ? error : new NetworkError())
    }
  })
}

async function refreshOnce(): Promise<void> {
  const epoch = sessionEpoch
  const token = refreshToken
  if (!token) {
    clearSession()
    throw new SessionExpiredError()
  }
  let envelope: Envelope<{ tokens: MiniTokenPair }>
  try {
    envelope = await send<{ tokens: MiniTokenPair }>('/auth/refresh', { refreshToken: token }, 'POST', false)
  }
  catch (error) {
    clearSession()
    if (error instanceof ApiError)
      throw new SessionExpiredError(error.message)
    throw new SessionExpiredError()
  }
  if (!envelope.success) {
    clearSession()
    throw new SessionExpiredError(envelope.error.message)
  }
  if (epoch !== sessionEpoch)
    throw new SessionExpiredError()
  try {
    applyTokens(envelope.data.tokens)
  }
  catch {
    clearSession()
    throw new SessionExpiredError()
  }
}

/** 单飞刷新：并发 401 只触发一次 refresh。 */
function refreshSession(): Promise<void> {
  refreshTask ??= refreshOnce().finally(() => {
    refreshTask = undefined
  })
  return refreshTask
}

async function call<T>(path: string, input: unknown, options: RequestOptions = {}): Promise<T> {
  const method = options.method ?? 'POST'
  const auth = options.auth ?? true
  const envelope = await send<T>(path, input, method, auth)
  if (envelope.success)
    return envelope.data
  if (envelope.error.httpStatus === 401 && auth && options.retryOnUnauthorized !== false) {
    await refreshSession()
    const retried = await send<T>(path, input, method, true)
    if (retried.success)
      return retried.data
    throw toApiError(retried.error, 401)
  }
  throw toApiError(envelope.error)
}

async function authenticate(path: string, body: Record<string, unknown>): Promise<MiniLoginResult> {
  clearSession()
  const envelope = await send<MiniLoginResult>(path, body, 'POST', false)
  if (!envelope.success)
    throw toApiError(envelope.error)
  applyTokens(envelope.data.tokens)
  cachedUser = { ...envelope.data.user, roles: envelope.data.roles, permissions: envelope.data.permissions }
  return envelope.data
}

/** 账号密码登录；成功后 access/refresh token 只保存在内存。 */
export async function login(identifier: string, password: string): Promise<MiniLoginResult> {
  return authenticate('/auth/login', { identifier, password })
}

/** LDAP 登录；仅在 `publicConfig.login.ldap` 为 true 时使用。 */
export async function loginWithLdap(username: string, password: string): Promise<MiniLoginResult> {
  return authenticate('/auth/login/ldap', { username, password })
}

/** 读取公开认证配置；用于决定登录页展示哪些入口。 */
export async function publicConfig(): Promise<MiniPublicConfig | null> {
  return call<MiniPublicConfig | null>('/auth/config', undefined, { method: 'GET', auth: false })
}

/** 读取当前用户并刷新内存用户摘要。 */
export async function currentUser(): Promise<MiniCurrentUser> {
  const user = await call<MiniCurrentUser>('/auth/me', undefined, { method: 'GET' })
  cachedUser = { ...user, roles: user.roles ?? [], permissions: user.permissions ?? [] }
  return cachedUser
}

/**
 * 退出登录：尽力撤销服务端会话，最终一定清理内存 token。
 * 不把网络失败暴露给用户，避免无法退出。
 */
export async function logout(): Promise<void> {
  const token = accessToken
  try {
    if (token)
      await call<unknown>('/auth/logout', { accessToken: token }, { retryOnUnauthorized: false })
  }
  catch {
    // 本地会话必须清理；服务端撤销失败不影响退出结果。
  }
  finally {
    clearSession()
  }
}

/** 更新个人资料。 */
export async function updateProfile(input: ProfileUpdate): Promise<AdminUser> {
  return call<AdminUser>('/profile/update', input)
}

/** 修改当前用户密码；成功页面会要求重新登录。 */
export async function changePassword(oldPassword: string, newPassword: string): Promise<unknown> {
  return call<unknown>('/auth/change-password', { oldPassword, newPassword })
}

/** SpaceGate 管理 API：输入/输出类型全部来自共享 contract，运行期仍是 wx.request。 */
export const spacegate = {
  config: {
    names: (): Promise<SpacegateNamesOutput> => call<SpacegateNamesOutput>('/spacegate/config/names', {}),
    get: (name: string): Promise<SpacegateConfigGetOutput> => call<SpacegateConfigGetOutput>('/spacegate/config/get', { name }),
    update: (input: SpacegateConfigUpdateInput): Promise<SpacegateVersionOutput> => call<SpacegateVersionOutput>('/spacegate/config/update', input),
  },
  gateway: {
    get: (name: string): Promise<SpacegateGatewayGetOutput> => call<SpacegateGatewayGetOutput>('/spacegate/gateway/get', { name }),
    upsert: (input: SpacegateGatewayUpsertInput): Promise<SpacegateVersionOutput> => call<SpacegateVersionOutput>('/spacegate/gateway/upsert', input),
    delete: (input: SpacegateGatewayDeleteInput): Promise<SpacegateVersionOutput> => call<SpacegateVersionOutput>('/spacegate/gateway/delete', input),
  },
  route: {
    list: (input: SpacegateRouteListInput): Promise<SpacegateRouteListOutput> => call<SpacegateRouteListOutput>('/spacegate/route/list', input),
    get: (gateway: string, route: string): Promise<SpacegateRouteGetOutput> => call<SpacegateRouteGetOutput>('/spacegate/route/get', { gateway, route }),
    upsert: (input: SpacegateRouteUpsertInput): Promise<SpacegateVersionOutput> => call<SpacegateVersionOutput>('/spacegate/route/upsert', input),
    rename: (input: SpacegateRouteRenameInput): Promise<SpacegateVersionOutput> => call<SpacegateVersionOutput>('/spacegate/route/rename', input),
    delete: (input: SpacegateRouteDeleteInput): Promise<SpacegateVersionOutput> => call<SpacegateVersionOutput>('/spacegate/route/delete', input),
  },
  plugin: {
    list: (input: SpacegatePluginListInput): Promise<SpacegateResourceListOutput> => call<SpacegateResourceListOutput>('/spacegate/plugin/list', input),
    get: (id: PluginInstanceId): Promise<SpacegatePluginGetOutput> => call<SpacegatePluginGetOutput>('/spacegate/plugin/get', { id }),
    upsert: (input: SpacegatePluginUpsertInput): Promise<SpacegateVersionOutput> => call<SpacegateVersionOutput>('/spacegate/plugin/upsert', input),
    delete: (input: SpacegatePluginDeleteInput): Promise<SpacegateVersionOutput> => call<SpacegateVersionOutput>('/spacegate/plugin/delete', input),
    catalog: (): Promise<SpacegateNamesOutput> => call<SpacegateNamesOutput>('/spacegate/plugin/catalog', {}),
    attrAll: (): Promise<SpacegateResourceListOutput> => call<SpacegateResourceListOutput>('/spacegate/plugin/attr-all', {}),
    attr: (code: string): Promise<SpacegatePluginAttrOutput> => call<SpacegatePluginAttrOutput>('/spacegate/plugin/attr', { code }),
    schema: (code: string): Promise<SpacegateSchemaOutput> => call<SpacegateSchemaOutput>('/spacegate/plugin/schema', { code }),
    wasmSchema: (id: PluginInstanceId): Promise<SpacegateSchemaOutput> => call<SpacegateSchemaOutput>('/spacegate/plugin/wasm-schema', { id }),
    previewWasmSchema: (input: SpacegatePluginPreviewWasmSchemaInput): Promise<SpacegateSchemaPreviewOutput> => call<SpacegateSchemaPreviewOutput>('/spacegate/plugin/preview-wasm-schema', input),
    refresh: (): Promise<SpacegateResourceListOutput> => call<SpacegateResourceListOutput>('/spacegate/plugin/refresh', {}),
  },
  discovery: {
    instances: (): Promise<SpacegateNamesOutput> => call<SpacegateNamesOutput>('/spacegate/discovery/instances', {}),
    health: (): Promise<SpacegateDiscoveryHealthOutput> => call<SpacegateDiscoveryHealthOutput>('/spacegate/discovery/health', {}),
    reloadGateway: (input: SpacegateDiscoveryReloadGatewayInput): Promise<SpacegateVersionOutput> => call<SpacegateVersionOutput>('/spacegate/discovery/reload-gateway', input),
    reloadGlobal: (input: SpacegateDiscoveryReloadGlobalInput): Promise<SpacegateVersionOutput> => call<SpacegateVersionOutput>('/spacegate/discovery/reload-global', input),
    reloadRoute: (input: SpacegateDiscoveryReloadRouteInput): Promise<SpacegateVersionOutput> => call<SpacegateVersionOutput>('/spacegate/discovery/reload-route', input),
    backends: (): Promise<SpacegateResourceListOutput> => call<SpacegateResourceListOutput>('/spacegate/discovery/backends', {}),
  },
  aiQueue: {
    schema: (plugin?: string): Promise<SpacegateAiQueueSchemaOutput> => call<SpacegateAiQueueSchemaOutput>('/spacegate/ai-queue/schema', { ...(plugin ? { plugin } : {}) }),
    readme: (plugin?: string): Promise<SpacegateAiQueueReadmeOutput> => call<SpacegateAiQueueReadmeOutput>('/spacegate/ai-queue/readme', { ...(plugin ? { plugin } : {}) }),
    rules: (input: SpacegateAiQueueRulesInput): Promise<SpacegateAiQueueRulesOutput> => call<SpacegateAiQueueRulesOutput>('/spacegate/ai-queue/rules', input),
    upsert: (input: SpacegateAiQueueRuleInput): Promise<{ rule: unknown }> => call<{ rule: unknown }>('/spacegate/ai-queue/upsert', input),
    delete: (input: SpacegateAiQueueRuleInput): Promise<SpacegateAiQueueDeleteOutput> => call<SpacegateAiQueueDeleteOutput>('/spacegate/ai-queue/delete', input),
    resolve: (input: SpacegateAiQueueResolveInput): Promise<SpacegateAiQueueResolveOutput> => call<SpacegateAiQueueResolveOutput>('/spacegate/ai-queue/resolve', input),
  },
} as const

/** 仅供测试：清理模块级 token/用户与基地址配置。 */
export function resetApiStateForTests(): void {
  clearSession()
  apiBaseUrl = ''
  refreshTask = undefined
}
