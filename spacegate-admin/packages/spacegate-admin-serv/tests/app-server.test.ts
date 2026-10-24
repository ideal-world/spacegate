import type { CoreConfig, HaiResult } from '@h-ai/core'
import type { TransportClient } from '@h-ai/crypto'
import type { AdminUser } from 'spacegate-admin-contract'
import { Buffer } from 'node:buffer'
import { mkdtemp, rm } from 'node:fs/promises'
import os from 'node:os'
import path from 'node:path'
import { audit } from '@h-ai/audit'
import { cache } from '@h-ai/cache'
import { core, err, HaiCommonError } from '@h-ai/core'
import { crypto } from '@h-ai/crypto'
import { iam } from '@h-ai/iam'
import { afterAll, beforeAll, describe, expect, it, vi } from 'vitest'
import { createServerApp } from '../src/app-server.js'
import { closeModules, initializeModules } from '../src/server/runtime.js'

const PREFIX = '/api/v1'
const TEST_PASSWORD = 'TemplateTest123!'
const PNG = 'iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAQAAAC1HAwCAAAAC0lEQVR42mP8/x8AAwMCAO+aSxkAAAAASUVORK5CYII='
let app: ReturnType<typeof createServerApp>
let transport: TransportClient
let storageRoot: string
let cookie = ''
let adminToken = ''
let adminId = ''
let userToken = ''
let user: AdminUser
let encryptedRequests = 0
let encryptedResponses = 0
interface LoginData { user: { id: string }, tokens: { accessToken: string, refreshToken?: string }, roles: string[] }

/** 使用真实加密协议与 HTTP 服务，同时保留浏览器 refresh cookie。 */
async function call<T>(route: string, input?: object, token?: string, method = 'POST'): Promise<HaiResult<T>> {
  const response = await transport.encryptedFetch(`http://localhost${PREFIX}${route}`, {
    method,
    headers: { 'content-type': 'application/json', ...(token ? { authorization: `Bearer ${token}` } : {}) },
    ...(input ? { body: JSON.stringify(input) } : {}),
  })
  return response.json() as Promise<HaiResult<T>>
}

beforeAll(async () => {
  storageRoot = await mkdtemp(path.join(os.tmpdir(), 'spacegate-admin-avatars-'))
  vi.stubEnv('HAI_DB_DATABASE', '":memory:"')
  vi.stubEnv('HAI_STORAGE_ROOT', storageRoot)
  vi.stubEnv('HAI_ADMIN_DEFAULT_PASSWORD', TEST_PASSWORD)
  core.init({ configDir: './config' })
  await initializeModules()
  const key = crypto.asymmetric.generateKeyPair()
  if (!key.success)
    throw new Error(key.error.message)
  vi.stubEnv('HAI_CRYPTO_TRANSPORT_SERVERKEYRING', JSON.stringify({ activeKeyId: 'test-v1', keys: { 'test-v1': key.data } }))
  app = createServerApp()
  transport = crypto.transport.createClient({
    keyExchangeUrl: `http://localhost${PREFIX}/_hai/key-exchange`,
    fetch: async (input, init) => {
      const request = new Request(input, init)
      if (cookie && request.url.endsWith('/auth/refresh'))
        request.headers.set('cookie', cookie)
      if (request.headers.get('x-encrypted') === 'true')
        encryptedRequests++
      const response = await app.fetch(request)
      if (response.headers.get('x-encrypted') === 'true') {
        encryptedResponses++
        const wire = await response.clone().text()
        expect(wire).not.toContain(TEST_PASSWORD)
        expect(wire).not.toContain('"username"')
      }
      const nextCookie = response.headers.get('set-cookie')
      if (nextCookie)
        cookie = nextCookie.split(';')[0] ?? ''
      return response
    },
  })
}, 30_000)

afterAll(async () => {
  transport?.destroy()
  await closeModules()
  vi.unstubAllEnvs()
  await rm(storageRoot, { recursive: true, force: true })
})

describe('spacegate-admin-serv mini administration', () => {
  it('keeps health public, requires encryption and reads runtime identity from core', async () => {
    expect((await app.request('/health')).status).toBe(200)
    expect((await app.request(`${PREFIX}/app/info`, { method: 'POST' })).status).toBe(400)
    const result = await call<{ name: string, version: string, env: string, defaultLocale: string }>('/app/info')
    expect(result.success).toBe(true)
    if (!result.success)
      return
    const config = core.config.getOrThrow<CoreConfig>('core')
    expect(result.data).toMatchObject({ name: config.name, version: config.version, env: config.env, defaultLocale: config.defaultLocale })
    const invalid = await transport.encryptedFetch(`http://localhost${PREFIX}/app/echo`, {
      method: 'POST',
      headers: { 'content-type': 'application/json' },
      body: JSON.stringify({ message: '' }),
    })
    expect(invalid.status).toBe(400)
  })

  it('logs in and refreshes with a browser cookie without exposing its refresh token', async () => {
    const result = await call<LoginData>('/auth/login', { identifier: 'admin', password: TEST_PASSWORD })
    expect(result.success).toBe(true)
    if (!result.success)
      return
    adminToken = result.data.tokens.accessToken
    adminId = result.data.user.id
    expect(result.data.roles).toEqual(['admin'])
    expect(result.data.tokens.refreshToken).toBeUndefined()
    expect(cookie).toContain('hai_refresh_token=')
    const refreshed = await call<{ tokens: { accessToken: string, refreshToken?: string } }>('/auth/refresh', {})
    expect(refreshed.success).toBe(true)
    if (refreshed.success) {
      expect(refreshed.data.tokens.refreshToken).toBeUndefined()
      adminToken = refreshed.data.tokens.accessToken
    }
  })

  it('mounts public auth routes while keeping default IAM administration routes private', async () => {
    const config = await transport.encryptedFetch(`http://localhost${PREFIX}/auth/config`)
    expect(config.status).toBe(200)
    for (const route of ['/auth/register', '/auth/login/ldap', '/auth/login/otp', '/auth/otp/send', '/auth/password-reset/request', '/auth/password-reset/confirm']) {
      const response = await transport.encryptedFetch(`http://localhost${PREFIX}${route}`, {
        method: 'POST',
        headers: { 'content-type': 'application/json' },
        body: '{}',
      })
      expect(response.status).toBe(400)
    }
    for (const [route, method] of [
      ['/iam/users', 'GET'],
      ['/iam/roles', 'POST'],
    ] as const) {
      const response = await transport.encryptedFetch(`http://localhost${PREFIX}${route}`, {
        method,
        headers: { 'content-type': 'application/json', 'authorization': `Bearer ${adminToken}` },
        ...(method === 'POST' ? { body: '{}' } : {}),
      })
      expect(response.status).toBe(404)
    }
  })

  it('creates a normal user and enforces guards on every administration API', async () => {
    const created = await call<AdminUser>('/admin/users/create', { username: 'normal-user', password: TEST_PASSWORD, role: 'user', enabled: true }, adminToken)
    expect(created.success).toBe(true)
    if (!created.success)
      return
    user = created.data
    const loggedIn = await call<LoginData>('/auth/login', { identifier: user.username, password: TEST_PASSWORD })
    expect(loggedIn.success).toBe(true)
    if (!loggedIn.success)
      return
    userToken = loggedIn.data.tokens.accessToken
    for (const [route, input] of [
      ['/admin/users/list', { page: 1, pageSize: 20 }],
      ['/admin/users/create', { username: 'forbidden', password: TEST_PASSWORD }],
      ['/admin/users/update', { id: adminId, enabled: false }],
      ['/admin/users/delete', { id: adminId }],
      ['/admin/users/avatar', { id: adminId, contentType: 'image/png', base64: PNG }],
      ['/admin/roles/list', undefined],
      ['/admin/roles/update', { id: 'missing', description: 'x' }],
      ['/admin/audit/list', { page: 1, pageSize: 20 }],
    ] as const) {
      const denied = await call(route, input, userToken)
      expect(denied.success).toBe(false)
      if (!denied.success)
        expect(denied.error.httpStatus).toBe(403)
    }
  })

  it('allows refresh-token body transport only for an identified trusted native origin', async () => {
    for (const [clientName, origin] of [['spacegate-admin-app', 'capacitor://localhost'], ['spacegate-admin-desktop', 'http://tauri.localhost']] as const) {
      const headers = { 'content-type': 'application/json', 'x-request-client': clientName, 'x-hai-token-transport': 'body', origin }
      const login = await transport.encryptedFetch(`http://localhost${PREFIX}/auth/login`, {
        method: 'POST',
        headers,
        body: JSON.stringify({ identifier: 'admin', password: TEST_PASSWORD }),
      })
      const result = await login.json() as HaiResult<LoginData>
      expect(result.success).toBe(true)
      if (!result.success)
        continue
      expect(result.data.tokens.refreshToken).toBeTruthy()
      expect(login.headers.get('set-cookie')).toBeNull()
      const refresh = await transport.encryptedFetch(`http://localhost${PREFIX}/auth/refresh`, {
        method: 'POST',
        headers,
        body: JSON.stringify({ refreshToken: result.data.tokens.refreshToken }),
      })
      const refreshed = await refresh.json() as HaiResult<{ tokens: { refreshToken: string } }>
      expect(refreshed.success).toBe(true)
      if (refreshed.success)
        expect(refreshed.data.tokens.refreshToken).not.toBe(result.data.tokens.refreshToken)
    }
    for (const [clientName, origin] of [['spacegate-admin-app', 'http://untrusted.example'], ['unrecognized-client', 'capacitor://localhost']] as const) {
      const response = await transport.encryptedFetch(`http://localhost${PREFIX}/auth/login`, {
        method: 'POST',
        headers: { 'content-type': 'application/json', 'x-request-client': clientName, 'x-hai-token-transport': 'body', origin },
        body: JSON.stringify({ identifier: 'admin', password: TEST_PASSWORD }),
      })
      const result = await response.json() as HaiResult<LoginData>
      expect(result.success).toBe(true)
      if (result.success)
        expect(result.data.tokens.refreshToken).toBeUndefined()
    }
  })

  it('paginates users and protects administrators from self deletion, disable and downgrade', async () => {
    const listed = await call<{ items: AdminUser[], total: number }>('/admin/users/list', { page: 1, pageSize: 1 }, adminToken)
    expect(listed.success).toBe(true)
    if (listed.success) {
      expect(listed.data.items).toHaveLength(1)
      expect(listed.data.total).toBe(2)
    }
    for (const [route, input] of [['/admin/users/delete', { id: adminId }], ['/admin/users/update', { id: adminId, enabled: false }], ['/admin/users/update', { id: adminId, role: 'user' }]] as const) {
      const denied = await call(route, input, adminToken)
      expect(denied.success).toBe(false)
      if (!denied.success)
        expect(denied.error.httpStatus).toBe(403)
    }
  })

  it('updates personal preferences, validates images and serves only allowed avatar resources', async () => {
    expect((await call('/profile/update', { displayName: 'Updated', locale: 'en-US', theme: 'light' }, userToken)).success).toBe(true)
    const current = await call<{ metadata: { locale: string, theme: string } }>('/auth/me', undefined, userToken, 'GET')
    expect(current.success).toBe(true)
    if (current.success)
      expect(current.data.metadata).toMatchObject({ locale: 'en-US', theme: 'light' })
    expect((await call('/profile/update', { email: 'temporary@example.com' }, userToken)).success).toBe(true)
    const cleared = await call<AdminUser>('/profile/update', { email: '' }, userToken)
    expect(cleared.success).toBe(true)
    if (cleared.success)
      expect(cleared.data.email).toBe('')
    const invalid = await call('/profile/avatar', { contentType: 'image/png', base64: 'PHN2Zz48L3N2Zz4=' }, userToken)
    expect(invalid.success).toBe(false)
    if (!invalid.success)
      expect(invalid.error.httpStatus).toBe(400)
    const oversized = await call('/profile/avatar', { contentType: 'image/png', base64: Buffer.alloc(512 * 1024 + 1).toString('base64') }, userToken)
    expect(oversized.success).toBe(false)
    const anonymous = await call('/profile/avatar', { contentType: 'image/png', base64: PNG })
    expect(anonymous.success).toBe(false)
    if (!anonymous.success)
      expect(anonymous.error.httpStatus).toBe(401)
    const injected = await transport.encryptedFetch(`http://localhost${PREFIX}/profile/update`, {
      method: 'POST',
      headers: { 'content-type': 'application/json', 'authorization': `Bearer ${userToken}` },
      body: JSON.stringify({ id: adminId, enabled: false, displayName: 'Intruder' }),
    })
    expect(injected.status).toBe(400)
    const uploaded = await call<{ avatarUrl: string }>('/profile/avatar', { contentType: 'image/png', base64: PNG }, userToken)
    expect(uploaded.success).toBe(true)
    if (!uploaded.success)
      return
    expect(uploaded.data.avatarUrl).toMatch(/^\/api\/v1\/assets\/avatars\/[\w-]+\/[\w-]+\.png$/)
    const asset = await app.request(uploaded.data.avatarUrl)
    expect(asset.status).toBe(200)
    expect(asset.headers.get('content-type')).toBe('image/png')
    expect((await asset.arrayBuffer()).byteLength).toBeGreaterThan(0)
    expect((await app.request(`${PREFIX}/assets/avatars/%2e%2e/config/_core.yml`)).status).toBe(404)
    expect((await call('/admin/users/avatar', { id: user.id, contentType: 'image/png', base64: PNG }, adminToken)).success).toBe(true)
    expect((await app.request(uploaded.data.avatarUrl)).status).toBe(404)
    const hostile = await transport.encryptedFetch(`http://attacker.example${PREFIX}/profile/avatar`, {
      method: 'POST',
      headers: { 'content-type': 'application/json', 'authorization': `Bearer ${userToken}` },
      body: JSON.stringify({ contentType: 'image/png', base64: PNG }),
    })
    const hostileResult = await hostile.json() as HaiResult<{ avatarUrl: string }>
    expect(hostileResult.success).toBe(true)
    if (hostileResult.success) {
      expect(hostileResult.data.avatarUrl).toMatch(/^\/api\/v1\/assets\//)
      expect(hostileResult.data.avatarUrl).not.toContain('attacker.example')
      const stored = await iam.user.getUser(user.id)
      expect(stored.success).toBe(true)
      if (stored.success)
        expect(stored.data?.avatarUrl).toBe(hostileResult.data.avatarUrl)
    }
  })

  it('lists two roles, updates descriptions and queries audit records without credentials or file content', async () => {
    const roles = await call<{ items: { id: string, code: string, userCount: number }[] }>('/admin/roles/list', undefined, adminToken)
    expect(roles.success).toBe(true)
    if (roles.success) {
      expect(roles.data.items.map(item => item.code).sort()).toEqual(['admin', 'user'])
      const role = roles.data.items.find(item => item.code === 'user')!
      expect(role.userCount).toBe(1)
      expect((await call('/admin/roles/update', { id: role.id, description: 'Updated description' }, adminToken)).success).toBe(true)
    }
    const logs = await call<{ items: { action: string, details: string | null }[], total: number }>('/admin/audit/list', { page: 1, pageSize: 100 }, adminToken)
    expect(logs.success).toBe(true)
    if (logs.success) {
      expect(logs.data.total).toBeGreaterThan(0)
      expect(JSON.stringify(logs.data)).not.toContain(TEST_PASSWORD)
      expect(JSON.stringify(logs.data)).not.toContain(PNG)
    }
  })

  it('does not write profile fields when role synchronization fails', async () => {
    const before = await iam.user.getUser(user.id)
    expect(before.success).toBe(true)
    const sync = vi.spyOn(iam.authz, 'syncRoles').mockResolvedValueOnce(err(HaiCommonError.API_ERROR, 'Injected role persistence failure'))
    const failed = await call('/admin/users/update', { id: user.id, role: 'admin', email: 'changed@example.com', displayName: 'Should not change' }, adminToken)
    sync.mockRestore()
    expect(failed.success).toBe(false)
    const after = await iam.user.getUser(user.id)
    expect(after.success).toBe(true)
    if (before.success && after.success)
      expect(after.data).toMatchObject({ email: before.data?.email, displayName: before.data?.displayName })
  })

  it('keeps an already committed profile update successful when best-effort audit fails', async () => {
    const log = vi.spyOn(audit, 'log').mockResolvedValueOnce(err(HaiCommonError.API_ERROR, 'Injected audit persistence failure'))
    const logger = vi.spyOn(core.logger, 'error').mockImplementation(() => {})
    try {
      const result = await call<AdminUser>('/profile/update', { displayName: 'Committed profile' }, userToken)
      expect(result.success).toBe(true)
      if (result.success)
        expect(result.data.displayName).toBe('Committed profile')
      expect(logger).toHaveBeenCalledWith('Failed to record audit event', expect.objectContaining({ action: 'update', resource: 'profile' }))
    }
    finally {
      log.mockRestore()
      logger.mockRestore()
    }
  })

  it('rolls back roles when profile persistence fails and keeps role changes effective in active sessions', async () => {
    const update = vi.spyOn(iam.user, 'updateUser').mockResolvedValueOnce(err(HaiCommonError.API_ERROR, 'Injected user persistence failure'))
    const failed = await call('/admin/users/update', { id: user.id, role: 'admin', email: 'changed@example.com' }, adminToken)
    update.mockRestore()
    expect(failed.success).toBe(false)
    const roles = await iam.authz.getUserRoles(user.id)
    expect(roles.success).toBe(true)
    if (roles.success)
      expect(roles.data.map(role => role.code)).toEqual(['user'])
    expect((await call('/admin/users/update', { id: user.id, role: 'admin' }, adminToken)).success).toBe(true)
    expect((await call('/admin/users/list', { page: 1, pageSize: 20 }, userToken)).success).toBe(true)
    expect((await call('/admin/users/update', { id: user.id, role: 'user' }, adminToken)).success).toBe(true)
    const denied = await call('/admin/users/list', { page: 1, pageSize: 20 }, userToken)
    expect(denied.success).toBe(false)
    if (!denied.success)
      expect(denied.error.httpStatus).toBe(403)
  })

  it('rejects stale administrator snapshots and reports fresh user roles', async () => {
    const snapshot = await iam.session.get(userToken)
    if (!snapshot.success || !snapshot.data)
      throw new Error('Session unavailable')
    const stale = await iam.session.update(userToken, { roles: ['admin'], permissions: ['*'] }, snapshot.data.version)
    expect(stale.success).toBe(true)
    for (const [route, input] of [['/admin/users/list', { page: 1, pageSize: 20 }], ['/admin/roles/list', undefined], ['/admin/audit/list', { page: 1, pageSize: 20 }]] as const) {
      const denied = await call(route, input, userToken)
      expect(denied.success).toBe(false)
      if (!denied.success)
        expect(denied.error.httpStatus).toBe(403)
    }
    const current = await call<{ roles: string[] }>('/auth/me', undefined, userToken, 'GET')
    expect(current.success).toBe(true)
    if (current.success)
      expect(current.data.roles).toEqual(['user'])
  })

  it('returns a conflict when another administrator holds the mutation lock and resumes after release', async () => {
    const locked = await cache.lock.acquire('hai:app:admin-mutation', { ttl: 10 })
    expect(locked.success).toBe(true)
    if (!locked.success || !locked.data)
      throw new Error('Mutation lock unavailable')
    const owner = locked.data
    try {
      const result = await call('/admin/users/update', { id: user.id, displayName: 'Locked' }, adminToken)
      expect(result.success).toBe(false)
      if (!result.success)
        expect(result.error.httpStatus).toBe(409)
    }
    finally {
      expect((await cache.lock.release('hai:app:admin-mutation', owner)).success).toBe(true)
    }
    expect((await call('/admin/users/update', { id: user.id, displayName: 'Unlocked' }, adminToken)).success).toBe(true)
  })

  it('keeps single-writer protection after an expired lease and resumes after a slow mutation', async () => {
    const other = await call<AdminUser>('/admin/users/create', { username: 'lease-admin', password: TEST_PASSWORD, role: 'admin' }, adminToken)
    expect(other.success).toBe(true)
    if (!other.success)
      return
    const otherLogin = await call<LoginData>('/auth/login', { identifier: 'lease-admin', password: TEST_PASSWORD })
    expect(otherLogin.success).toBe(true)
    if (!otherLogin.success)
      return
    let resume: () => void = () => {}
    let entered: () => void = () => {}
    const paused = new Promise<void>((resolve) => {
      resume = resolve
    })
    const entry = new Promise<void>((resolve) => {
      entered = resolve
    })
    const original = iam.user.updateUser
    const update = vi.spyOn(iam.user, 'updateUser').mockImplementationOnce(async (id, data) => {
      entered()
      await paused
      return original(id, data)
    })
    const first = call('/admin/users/update', { id: other.data.id, enabled: false }, adminToken)
    await entry
    const now = vi.spyOn(Date, 'now').mockReturnValue(Date.now() + 301_000)
    try {
      const conflicting = await call('/admin/users/update', { id: adminId, enabled: false }, otherLogin.data.tokens.accessToken)
      expect(conflicting.success).toBe(false)
      if (!conflicting.success)
        expect(conflicting.error.httpStatus).toBe(409)
      resume()
      expect((await first).success).toBe(true)
      const admin = await iam.user.getUser(adminId)
      expect(admin.success).toBe(true)
      if (admin.success)
        expect(admin.data?.enabled).toBe(true)
    }
    finally {
      resume()
      now.mockRestore()
      update.mockRestore()
    }
    expect((await call('/admin/users/delete', { id: other.data.id }, adminToken)).success).toBe(true)
  })

  it('serializes mutually destructive admin requests and preserves an enabled administrator', async () => {
    const created = await call<AdminUser>('/admin/users/create', { username: 'second-admin', password: TEST_PASSWORD, role: 'admin' }, adminToken)
    expect(created.success).toBe(true)
    if (!created.success)
      return
    const loggedIn = await call<LoginData>('/auth/login', { identifier: 'second-admin', password: TEST_PASSWORD })
    expect(loggedIn.success).toBe(true)
    if (!loggedIn.success)
      return
    // 对手同时请求禁用原管理员；锁内重新鉴权阻止两个请求全部成功。
    const results = await Promise.all([
      call('/admin/users/update', { id: created.data.id, enabled: false }, adminToken),
      call('/admin/users/update', { id: adminId, enabled: false }, loggedIn.data.tokens.accessToken),
    ])
    expect(results.filter(result => result.success)).toHaveLength(1)
    const admins = await iam.user.listUsers({ enabled: true, include: ['roles'] })
    expect(admins.success).toBe(true)
    if (admins.success) {
      const surviving = admins.data.items.filter(item => item.roles?.some(role => role.code === 'admin'))
      expect(surviving).toHaveLength(1)
      // 后续用仍有效的管理员会话验证其他操作。
      if (surviving[0]?.id !== adminId) {
        adminToken = loggedIn.data.tokens.accessToken
        adminId = created.data.id
      }
    }
  })

  it('invalidates sessions on disable and password change and proves encrypted traffic', async () => {
    expect((await call('/admin/users/update', { id: user.id, enabled: false }, adminToken)).success).toBe(true)
    const denied = await call('/auth/me', undefined, userToken, 'GET')
    expect(denied.success).toBe(false)
    if (!denied.success)
      expect(denied.error.httpStatus).toBe(401)
    expect((await call('/admin/users/update', { id: user.id, enabled: true }, adminToken)).success).toBe(true)
    const loggedIn = await call<LoginData>('/auth/login', { identifier: user.username, password: TEST_PASSWORD })
    expect(loggedIn.success).toBe(true)
    if (!loggedIn.success)
      return
    expect((await call('/auth/change-password', { oldPassword: TEST_PASSWORD, newPassword: 'ChangedTest123!' }, loggedIn.data.tokens.accessToken)).success).toBe(true)
    expect((await call('/auth/me', undefined, loggedIn.data.tokens.accessToken, 'GET')).success).toBe(false)
    const changedLogin = await call<LoginData>('/auth/login', { identifier: user.username, password: 'ChangedTest123!' })
    expect(changedLogin.success).toBe(true)
    if (changedLogin.success) {
      expect((await call('/auth/logout', {}, changedLogin.data.tokens.accessToken)).success).toBe(true)
      expect((await call('/auth/me', undefined, changedLogin.data.tokens.accessToken, 'GET')).success).toBe(false)
    }
    expect((await call('/admin/users/delete', { id: user.id }, adminToken)).success).toBe(true)
    const removed = await iam.user.getUser(user.id)
    expect(removed.success).toBe(true)
    if (removed.success)
      expect(removed.data).toBeNull()
    expect(encryptedRequests).toBeGreaterThan(10)
    expect(encryptedResponses).toBeGreaterThan(10)
  })
})
