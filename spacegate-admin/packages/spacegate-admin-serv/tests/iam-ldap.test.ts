import type { HaiResult } from '@h-ai/core'
import type { TransportClient } from '@h-ai/crypto'
import type { LdapClientFactory } from '@h-ai/iam'
import type { AdminUser } from 'spacegate-admin-contract'
import { mkdtemp, rm } from 'node:fs/promises'
import os from 'node:os'
import path from 'node:path'
import { audit } from '@h-ai/audit'
import { core, err, ok } from '@h-ai/core'
import { crypto } from '@h-ai/crypto'
import { HaiIamError, iam, IamConfigSchema } from '@h-ai/iam'
import { afterAll, beforeAll, beforeEach, describe, expect, it, vi } from 'vitest'
import { createServerApp } from '../src/app-server.js'
import { closeModules, initializeModules } from '../src/server/runtime.js'

const PREFIX = '/api/v1'
const TEST_PASSWORD = 'LdapTest123!'
const LDAP_BIND_PASSWORD = 'LdapBindSecret456!'
const LDAP_USER_PASSWORD = 'LdapUserSecret789!'
const CHANGED_PASSWORD = 'LdapChanged123!'

interface LoginData {
  user: { id: string, username: string }
  tokens: { accessToken: string, refreshToken?: string }
  roles: string[]
}

interface LdapEntryFixture {
  dn: string
  username: string
  email?: string
  displayName?: string
}

interface LdapFixture {
  entries?: LdapEntryFixture[]
  adminBindFails?: boolean
  searchFails?: boolean
  connectionFails?: boolean
  throws?: boolean
}

let app: ReturnType<typeof createServerApp>
let transport: TransportClient
let storageRoot: string
let cookie = ''
let activeFactory: LdapClientFactory

/** 默认 LDAP 客户端工厂只由测试注入；任何分支都不会访问真实目录服务。 */
const injectedLdapFactory: LdapClientFactory = config => activeFactory(config)

/**
 * 构造可预测的 LDAP 客户端：绑定账号、检索和用户绑定均可独立失败。
 * @param fixture - 本次登录期望的目录条目与失败分支
 * @returns 返回固定 HaiResult 的客户端工厂
 */
function createLdapFactory(fixture: LdapFixture): LdapClientFactory {
  return async (config) => {
    if (fixture.throws)
      throw new Error('LDAP endpoint unavailable')
    if (fixture.connectionFails)
      return err(HaiIamError.LDAP_CONNECTION_FAILED)
    return ok({
      async bind(dn, password) {
        if (dn === config.bindDn)
          return fixture.adminBindFails ? err(HaiIamError.LDAP_BIND_FAILED) : ok(undefined)
        if (password !== LDAP_USER_PASSWORD)
          return err(HaiIamError.LDAP_BIND_FAILED)
        return ok(undefined)
      },
      async search() {
        if (fixture.searchFails)
          return err(HaiIamError.LDAP_SEARCH_FAILED)
        return ok((fixture.entries ?? []).map(entry => ({
          dn: entry.dn,
          attributes: {
            [config.usernameAttribute]: entry.username,
            [config.emailAttribute]: entry.email ?? `${entry.username}@example.com`,
            [config.displayNameAttribute]: entry.displayName ?? entry.username,
          },
        })))
      },
      async unbind() {
        return ok(undefined)
      },
    })
  }
}

/** 使用真实加密协议与 HTTP 服务，同时保留浏览器 refresh cookie。 */
async function call<T>(route: string, input?: object, token?: string, method = 'POST'): Promise<HaiResult<T>> {
  const response = await transport.encryptedFetch(`http://localhost${PREFIX}${route}`, {
    method,
    headers: { 'content-type': 'application/json', ...(token ? { authorization: `Bearer ${token}` } : {}) },
    ...(input ? { body: JSON.stringify(input) } : {}),
  })
  return response.json() as Promise<HaiResult<T>>
}

describe('iam password/LDAP, role enforcement and audit', () => {
  beforeAll(async () => {
    activeFactory = createLdapFactory({})
    storageRoot = await mkdtemp(path.join(os.tmpdir(), 'spacegate-admin-ldap-'))
    vi.stubEnv('HAI_DB_DATABASE', '":memory:"')
    vi.stubEnv('HAI_STORAGE_ROOT', storageRoot)
    vi.stubEnv('HAI_ADMIN_DEFAULT_PASSWORD', TEST_PASSWORD)
    core.init({ configDir: './config' })
    await initializeModules()

    // runtime 已创建角色与首个管理员；重新初始化 IAM 以启用 LDAP 并注入测试工厂。
    // 从原始配置重新解析，避免读取 iam.config 时敏感键已被脱敏。
    const config = IamConfigSchema.parse(core.config.getOrThrow('iam'))
    const reinitialized = await iam.init({
      ...config,
      login: { ...config.login, ldap: true },
      // IAM 的 LDAP 自动同步沿用 register.defaultEnabled；启用 LDAP 的部署需同时打开该开关。
      register: { ...config.register, defaultEnabled: true },
      ldap: {
        url: 'ldap://127.0.0.1:389',
        bindDn: 'cn=service,dc=example,dc=com',
        bindPassword: LDAP_BIND_PASSWORD,
        searchBase: 'ou=users,dc=example,dc=com',
        searchFilter: '(uid={{username}})',
        usernameAttribute: 'uid',
        emailAttribute: 'mail',
        displayNameAttribute: 'cn',
        useTls: false,
        connectTimeout: 5000,
      },
      ldapClientFactory: injectedLdapFactory,
    })
    if (!reinitialized.success)
      throw new Error(reinitialized.error.message)

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
        const response = await app.fetch(request)
        const nextCookie = response.headers.get('set-cookie')
        if (nextCookie)
          cookie = nextCookie.split(';')[0] ?? ''
        return response
      },
    })
  }, 30_000)

  beforeEach(() => {
    cookie = ''
    activeFactory = createLdapFactory({})
  })

  afterAll(async () => {
    transport?.destroy()
    await closeModules()
    vi.unstubAllEnvs()
    await rm(storageRoot, { recursive: true, force: true })
  })

  it('keeps password login and refresh-cookie sessions working', async () => {
    const login = await call<LoginData>('/auth/login', { identifier: 'admin', password: TEST_PASSWORD })
    expect(login.success).toBe(true)
    if (!login.success)
      return
    expect(login.data.roles).toEqual(['admin'])
    expect(login.data.tokens.accessToken).toBeTruthy()

    const refreshed = await call<{ tokens: { accessToken: string } }>('/auth/refresh', {})
    expect(refreshed.success).toBe(true)
    if (!refreshed.success)
      return
    const me = await call<{ username: string }>('/auth/me', undefined, refreshed.data.tokens.accessToken, 'GET')
    expect(me.success).toBe(true)
    if (me.success)
      expect(me.data.username).toBe('admin')
  })

  it('synchronizes a first LDAP login as user, refreshes tokens and requires an explicit admin grant', async () => {
    activeFactory = createLdapFactory({ entries: [{ dn: 'uid=alice,ou=users,dc=example,dc=com', username: 'ldap-alice', email: 'alice@example.com', displayName: 'Alice' }] })
    const login = await call<LoginData>('/auth/login/ldap', { username: 'alice', password: LDAP_USER_PASSWORD })
    expect(login.success).toBe(true)
    if (!login.success)
      return
    expect(login.data.user.username).toBe('ldap-alice')
    expect(login.data.roles).toEqual(['user'])

    const stored = await iam.user.getUser(login.data.user.id, { include: ['roles'] })
    expect(stored.success).toBe(true)
    if (stored.success) {
      expect(stored.data?.roles?.map(role => role.code)).toEqual(['user'])
      expect(stored.data?.metadata).toMatchObject({ authSource: 'ldap' })
    }

    // 普通用户不能调用任何 SpaceGate 管理 procedure；守卫在访问 Rust 上游前拒绝。
    const denied = await call('/spacegate/config/names', {}, login.data.tokens.accessToken)
    expect(denied.success).toBe(false)
    if (!denied.success)
      expect(denied.error.httpStatus).toBe(403)

    const refreshed = await call<{ tokens: { accessToken: string } }>('/auth/refresh', {})
    expect(refreshed.success).toBe(true)
    if (refreshed.success)
      expect((await call('/auth/me', undefined, refreshed.data.tokens.accessToken, 'GET')).success).toBe(true)

    const admin = await call<LoginData>('/auth/login', { identifier: 'admin', password: TEST_PASSWORD })
    expect(admin.success).toBe(true)
    if (!admin.success)
      return
    const granted = await call('/admin/users/update', { id: login.data.user.id, role: 'admin' }, admin.data.tokens.accessToken)
    expect(granted.success).toBe(true)

    const promoted = await call<LoginData>('/auth/login/ldap', { username: 'alice', password: LDAP_USER_PASSWORD })
    expect(promoted.success).toBe(true)
    if (promoted.success)
      expect(promoted.data.roles).toEqual(['admin'])

    expect((await call('/admin/users/update', { id: login.data.user.id, role: 'user' }, admin.data.tokens.accessToken)).success).toBe(true)
  })

  it('maps an LDAP bind failure to LDAP_BIND_FAILED without creating a local user', async () => {
    activeFactory = createLdapFactory({ adminBindFails: true, entries: [{ dn: 'uid=bind-failure,ou=users,dc=example,dc=com', username: 'bind-failure' }] })
    const result = await call('/auth/login/ldap', { username: 'bind-failure', password: LDAP_USER_PASSWORD })
    expect(result.success).toBe(false)
    if (!result.success)
      expect(result.error.code).toBe(HaiIamError.LDAP_BIND_FAILED.code)
    const users = await iam.user.listUsers({ search: 'bind-failure', page: 1, pageSize: 10 })
    expect(users.success).toBe(true)
    if (users.success)
      expect(users.data.total).toBe(0)
  })

  it('maps an LDAP search failure to LDAP_SEARCH_FAILED', async () => {
    activeFactory = createLdapFactory({ searchFails: true, entries: [{ dn: 'uid=search-failure,ou=users,dc=example,dc=com', username: 'search-failure' }] })
    const result = await call('/auth/login/ldap', { username: 'search-failure', password: LDAP_USER_PASSWORD })
    expect(result.success).toBe(false)
    if (!result.success)
      expect(result.error.code).toBe(HaiIamError.LDAP_SEARCH_FAILED.code)
    const users = await iam.user.listUsers({ search: 'search-failure', page: 1, pageSize: 10 })
    expect(users.success).toBe(true)
    if (users.success)
      expect(users.data.total).toBe(0)
  })

  it('maps returned and thrown LDAP connection failures to LDAP_CONNECTION_FAILED', async () => {
    activeFactory = createLdapFactory({ connectionFails: true })
    const returned = await call('/auth/login/ldap', { username: 'connection-failure', password: LDAP_USER_PASSWORD })
    expect(returned.success).toBe(false)
    if (!returned.success)
      expect(returned.error.code).toBe(HaiIamError.LDAP_CONNECTION_FAILED.code)

    activeFactory = createLdapFactory({ throws: true })
    const thrown = await call('/auth/login/ldap', { username: 'connection-throw', password: LDAP_USER_PASSWORD })
    expect(thrown.success).toBe(false)
    if (!thrown.success)
      expect(thrown.error.code).toBe(HaiIamError.LDAP_CONNECTION_FAILED.code)
  })

  it('writes audit rows for password login, LDAP login, logout, password change and admin writes', async () => {
    const admin = await call<LoginData>('/auth/login', { identifier: 'admin', password: TEST_PASSWORD })
    expect(admin.success).toBe(true)
    if (!admin.success)
      return

    activeFactory = createLdapFactory({ entries: [{ dn: 'uid=audit-ldap,ou=users,dc=example,dc=com', username: 'audit-ldap' }] })
    const ldapLogin = await call<LoginData>('/auth/login/ldap', { username: 'audit-ldap', password: LDAP_USER_PASSWORD })
    expect(ldapLogin.success).toBe(true)
    if (!ldapLogin.success)
      return

    const created = await call<AdminUser>('/admin/users/create', { username: 'audit-local', password: TEST_PASSWORD, role: 'user' }, admin.data.tokens.accessToken)
    expect(created.success).toBe(true)
    if (!created.success)
      return

    const localLogin = await call<LoginData>('/auth/login', { identifier: 'audit-local', password: TEST_PASSWORD })
    expect(localLogin.success).toBe(true)
    if (!localLogin.success)
      return
    expect((await call('/auth/logout', {}, localLogin.data.tokens.accessToken)).success).toBe(true)

    const relogin = await call<LoginData>('/auth/login', { identifier: 'audit-local', password: TEST_PASSWORD })
    expect(relogin.success).toBe(true)
    if (!relogin.success)
      return
    expect((await call('/auth/change-password', { oldPassword: TEST_PASSWORD, newPassword: CHANGED_PASSWORD }, relogin.data.tokens.accessToken)).success).toBe(true)

    const logs = await audit.list({ page: 1, pageSize: 200 })
    expect(logs.success).toBe(true)
    if (!logs.success)
      return
    const items = logs.data.items
    expect(items.some(item => item.action === 'login' && item.resource === 'auth' && item.userId === admin.data.user.id)).toBe(true)
    expect(items.some(item => item.action === 'create' && item.resource === 'users' && item.resourceId === created.data.id)).toBe(true)
    expect(items.some(item => item.action === 'logout' && item.resource === 'auth' && item.userId === created.data.id)).toBe(true)
    expect(items.some(item => item.action === 'password-change' && item.resource === 'profile' && item.userId === created.data.id)).toBe(true)
    const ldapAudit = items.find(item => item.action === 'login' && item.resource === 'auth' && item.userId === ldapLogin.data.user.id)
    expect(ldapAudit?.details).toBeTruthy()
    expect(JSON.parse(ldapAudit?.details ?? '{}')).toEqual({ strategy: 'ldap' })

    const serialized = JSON.stringify(items)
    expect(serialized).not.toContain(TEST_PASSWORD)
    expect(serialized).not.toContain(CHANGED_PASSWORD)
    expect(serialized).not.toContain(LDAP_BIND_PASSWORD)
  })
})

describe('bootstrap administrator', () => {
  let dataRoot: string

  beforeAll(async () => {
    dataRoot = await mkdtemp(path.join(os.tmpdir(), 'spacegate-admin-bootstrap-existing-'))
    vi.stubEnv('HAI_DB_DATABASE', JSON.stringify(path.join(dataRoot, 'admin.sqlite')))
    vi.stubEnv('HAI_STORAGE_ROOT', dataRoot)
    vi.stubEnv('HAI_ADMIN_DEFAULT_PASSWORD', 'FirstAdmin123!')
    core.init({ configDir: './config' })
    await initializeModules()
    await closeModules()
    vi.stubEnv('HAI_ADMIN_DEFAULT_PASSWORD', 'SecondAdmin456!')
    await initializeModules()
  }, 30_000)

  afterAll(async () => {
    await closeModules()
    vi.unstubAllEnvs()
    await rm(dataRoot, { recursive: true, force: true })
  })

  it('does not reset the bootstrap administrator when users already exist', async () => {
    const users = await iam.user.listUsers({ page: 1, pageSize: 10 })
    expect(users.success).toBe(true)
    if (users.success) {
      expect(users.data.total).toBe(1)
      expect(users.data.items[0]?.username).toBe('admin')
    }

    const first = await iam.auth.login({ identifier: 'admin', password: 'FirstAdmin123!' })
    expect(first.success).toBe(true)
    if (first.success)
      expect((await iam.auth.logout(first.data.tokens.accessToken)).success).toBe(true)

    const second = await iam.auth.login({ identifier: 'admin', password: 'SecondAdmin456!' })
    expect(second.success).toBe(false)
    if (!second.success)
      expect(second.error.code).toBe(HaiIamError.INVALID_CREDENTIALS.code)
  })
})
