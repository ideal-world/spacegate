import type { TokenStorage } from '@h-ai/api-client'
import type { AppInfoOutputData } from 'spacegate-admin-contract'
import { apiClient, HaiApiClientError } from '@h-ai/api-client'
import { err } from '@h-ai/core'
import { crypto } from '@h-ai/crypto'
import { spacegateAdminContract } from 'spacegate-admin-contract'

export type AppInfo = AppInfoOutputData
export const managementApi = apiClient.create(spacegateAdminContract)

const DEFAULT_API_ORIGIN = 'http://127.0.0.1:3000'

export interface ManagementApiOptions {
  fetchFn?: typeof fetch
  baseUrl?: string
  clientName?: string
  tokenStorage?: TokenStorage
  headers?: Record<string, string>
  requireApiBase?: boolean
  onSessionExpired?: () => void
}

/** 所有业务请求由 api-client 统一完成 Token 注入、刷新和双向加密。 */
export async function initializeManagementApi(options: ManagementApiOptions = {}) {
  if (options.requireApiBase && !options.baseUrl && !resolveRuntimeApiOrigin() && !import.meta.env.PUBLIC_API_BASE)
    return err(HaiApiClientError.CONFIG_ERROR, 'Native applications require a configured API origin')
  if (managementApi.isInitialized)
    await managementApi.close()
  if (!crypto.isInitialized) {
    const result = await crypto.init()
    if (!result.success)
      return result
  }

  return managementApi.init({
    baseUrl: normalizeApiBaseUrl(options.baseUrl ?? resolveDefaultApiOrigin()),
    clientName: options.clientName ?? 'spacegate-admin-shared',
    fetch: createCredentialedFetch(options.fetchFn),
    headers: options.headers,
    auth: { storage: options.tokenStorage, onRefreshFailed: options.onSessionExpired },
    transport: { crypto },
  })
}

/** 头像仅允许服务端约定资源路径，并统一指向当前可信 API origin。 */
export function resolveAvatarUrl(value: string | undefined): string | undefined {
  if (!value)
    return undefined
  try {
    const origin = managementApi.config?.baseUrl ?? normalizeApiBaseUrl(resolveDefaultApiOrigin())
    const parsed = new URL(value, origin)
    const prefix = `${new URL(origin).pathname.replace(/\/+$/, '')}/assets/avatars/`
    if (!parsed.pathname.startsWith(prefix) || !/^[\w-]+\/[\w-]+\.(?:png|jpg|webp)$/.test(parsed.pathname.slice(prefix.length)))
      return undefined
    return new URL(parsed.pathname, origin).toString()
  }
  catch {
    return undefined
  }
}

function resolveDefaultApiOrigin(): string {
  return resolveRuntimeApiOrigin()
    || import.meta.env.PUBLIC_API_BASE
    || (typeof globalThis.location === 'undefined' ? DEFAULT_API_ORIGIN : globalThis.location.origin)
}

/** 跨端浏览器请求携带服务端管理的 httpOnly refresh Cookie。 */
function createCredentialedFetch(fetchFn: typeof fetch = globalThis.fetch.bind(globalThis)): typeof fetch {
  return (input, init) => {
    const request = new Request(input, init)
    return fetchFn(new Request(request, { credentials: 'include' }))
  }
}

function resolveRuntimeApiOrigin(): string | undefined {
  const runtimeConfig = (globalThis as typeof globalThis & {
    __HAI_PUBLIC_CONFIG__?: { apiBase?: string }
  }).__HAI_PUBLIC_CONFIG__
  return runtimeConfig?.apiBase
}

function normalizeApiBaseUrl(value: string): string {
  const url = new URL(value.trim())
  if (url.pathname === '/')
    url.pathname = '/api/v1'
  return url.toString().replace(/\/+$/, '')
}

/** 标准 IAM 表单动作，平台安全 TokenStorage 由原有初始化配置决定。 */
export const managementAuthActions = apiClient.createAuthActions(managementApi)
