import type { HaiResult } from '@h-ai/core'
import { mkdtemp, rm } from 'node:fs/promises'
import os from 'node:os'
import path from 'node:path'
import { core } from '@h-ai/core'
import { crypto } from '@h-ai/crypto'
import { afterAll, beforeAll, describe, expect, it, vi } from 'vitest'
import { createMiniappServerApp, createServerApp, MINIAPP_API_PREFIX, MINIAPP_REQUEST_CLIENT } from '../src/app-server.js'
import { closeModules, initializeModules } from '../src/server/runtime.js'

const TEST_PASSWORD = 'TemplateTest123!'
const MAIN_PREFIX = '/api/v1'
interface LoginData { user: { id: string, username: string }, tokens: { accessToken: string, refreshToken?: string }, roles: string[] }
let miniapp: ReturnType<typeof createMiniappServerApp>
let main: ReturnType<typeof createServerApp>
let storageRoot: string

/** 小程序业务请求的最小头集合：客户端标识 + body token 通道。 */
const miniappHeaders = {
  'content-type': 'application/json',
  'x-request-client': MINIAPP_REQUEST_CLIENT,
  'x-hai-token-transport': 'body',
}

async function post<T>(app: { request: (input: RequestInfo | URL, init?: RequestInit) => Response | Promise<Response> }, prefix: string, route: string, body: unknown, extra: Record<string, string> = {}): Promise<{ response: Response, result: HaiResult<T> }> {
  const response = await app.request(`${prefix}${route}`, {
    method: 'POST',
    headers: { ...miniappHeaders, ...extra },
    body: JSON.stringify(body),
  })
  return { response, result: await response.json() as HaiResult<T> }
}

beforeAll(async () => {
  storageRoot = await mkdtemp(path.join(os.tmpdir(), 'spacegate-admin-miniapp-'))
  vi.stubEnv('HAI_DB_DATABASE', '":memory:"')
  vi.stubEnv('HAI_STORAGE_ROOT', storageRoot)
  vi.stubEnv('HAI_ADMIN_DEFAULT_PASSWORD', TEST_PASSWORD)
  core.init({ configDir: './config' })
  await initializeModules()
  const key = crypto.asymmetric.generateKeyPair()
  if (!key.success)
    throw new Error(key.error.message)
  vi.stubEnv('HAI_CRYPTO_TRANSPORT_SERVERKEYRING', JSON.stringify({ activeKeyId: 'test-v1', keys: { 'test-v1': key.data } }))
  miniapp = createMiniappServerApp()
  main = createServerApp()
}, 30_000)

afterAll(async () => {
  await closeModules()
  vi.unstubAllEnvs()
  await rm(storageRoot, { recursive: true, force: true })
})

describe('miniapp HTTP channel', () => {
  it('serves unencrypted JSON only on the dedicated miniapp app', async () => {
    const health = await miniapp.request('/health')
    expect(health.status).toBe(200)
    expect(health.headers.get('x-encrypted')).toBeNull()

    const { response, result } = await post<LoginData>(miniapp, MINIAPP_API_PREFIX, '/auth/login', { identifier: 'admin', password: TEST_PASSWORD })
    expect(response.status).toBe(200)
    expect(response.headers.get('x-encrypted')).toBeNull()
    expect(result.success).toBe(true)
    if (!result.success)
      return
    expect(result.data.tokens.accessToken).toBeTruthy()
    // 小程序刷新凭证只出现在 body，绝不下发浏览器 Cookie。
    expect(result.data.tokens.refreshToken).toBeTruthy()
    expect(response.headers.get('set-cookie')).toBeNull()

    // 主通道仍强制传输加密：同样的明文请求无法通过。
    const plainMain = await main.request(`${MAIN_PREFIX}/auth/login`, {
      method: 'POST',
      headers: { 'content-type': 'application/json' },
      body: JSON.stringify({ identifier: 'admin', password: TEST_PASSWORD }),
    })
    expect(plainMain.status).toBe(400)
    expect(plainMain.headers.get('x-encrypted')).toBeNull()
  })

  it('rotates the refresh token through the body channel', async () => {
    const login = await post<LoginData>(miniapp, MINIAPP_API_PREFIX, '/auth/login', { identifier: 'admin', password: TEST_PASSWORD })
    expect(login.result.success).toBe(true)
    if (!login.result.success)
      return
    const previous = login.result.data.tokens.refreshToken
    expect(previous).toBeTruthy()
    if (!previous)
      return
    const refreshed = await post<{ tokens: { accessToken: string, refreshToken?: string } }>(miniapp, MINIAPP_API_PREFIX, '/auth/refresh', { refreshToken: previous })
    expect(refreshed.result.success).toBe(true)
    if (!refreshed.result.success)
      return
    expect(refreshed.result.data.tokens.accessToken).toBeTruthy()
    expect(refreshed.result.data.tokens.refreshToken).toBeTruthy()
    expect(refreshed.result.data.tokens.refreshToken).not.toBe(previous)
  })

  it('rejects unauthenticated business routes and unknown clients', async () => {
    const denied = await post(miniapp, MINIAPP_API_PREFIX, '/spacegate/config/names', {})
    expect(denied.result.success).toBe(false)
    if (!denied.result.success)
      expect(denied.result.error.httpStatus).toBe(401)

    const missingClient = await miniapp.request(`${MINIAPP_API_PREFIX}/auth/login`, {
      method: 'POST',
      headers: { 'content-type': 'application/json', 'x-hai-token-transport': 'body' },
      body: JSON.stringify({ identifier: 'admin', password: TEST_PASSWORD }),
    })
    expect(missingClient.status).toBe(403)
    const body = await missingClient.json() as { success: boolean, error: { code: string } }
    expect(body.success).toBe(false)
    expect(body.error.code).toBe('MINIAPP_CLIENT_REQUIRED')
  })
})
