import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { HashRouter } from '../src/lib/navigation/hash-router.svelte.js'
import * as m from '../src/lib/paraglide/messages.js'
import { ManagementStore } from '../src/lib/stores/management-store.svelte.js'

const api = vi.hoisted(() => ({
  auth: { clear: vi.fn() },
  iam: { auth: { logout: vi.fn(), currentUser: vi.fn() } },
  admin: { users: { list: vi.fn() } },
}))
vi.mock('../src/lib/api/api-client.js', () => ({
  managementApi: api,
  initializeManagementApi: vi.fn(),
  resolveAvatarUrl: (url: string | undefined) => url,
}))

const member = {
  id: 'member-id',
  username: 'member',
  enabled: true,
  roles: ['user'],
  permissions: [],
  createdAt: '2026-01-01T00:00:00.000Z',
  updatedAt: '2026-01-01T00:00:00.000Z',
}
const administrator = { ...member, id: 'admin-id', username: 'admin', roles: ['admin'] }
const forbidden = { success: false, error: { code: 'test:forbidden', message: 'Forbidden', httpStatus: 403 } } as const

function deferred<T>() {
  let resolve!: (result: T) => void
  const promise = new Promise<T>((done) => {
    resolve = done
  })
  return { promise, resolve }
}

beforeEach(() => {
  vi.resetAllMocks()
  vi.stubGlobal('location', { hash: '#/users' })
  vi.stubGlobal('history', { replaceState: vi.fn(), pushState: vi.fn() })
  api.iam.auth.logout.mockResolvedValue({ success: true })
  api.iam.auth.currentUser.mockResolvedValue({ success: true, data: member })
})

afterEach(() => vi.unstubAllGlobals())

/** 会话测试使用真实路由状态，浏览器历史由 E2E 验证。 */
function createStore() {
  return new ManagementStore(new HashRouter(['../pages/users.svelte', '../pages/settings.svelte'], '/users'))
}

describe('management session and request ownership', () => {
  it('ignores a previous administrator failure after logout and another user login', async () => {
    const oldRequest = deferred<typeof forbidden>()
    api.admin.users.list.mockReturnValueOnce(oldRequest.promise)
    const store = createStore()
    store.user = administrator
    const loading = store.loadUsers()
    await store.logout()
    await store.authenticated()
    oldRequest.resolve(forbidden)
    await loading
    expect(store.user?.id).toBe(member.id)
    expect(store.page).toBe('settings')
    expect(store.error).toBe('')
    expect(store.loading).toBe(false)
    expect(api.iam.auth.currentUser).toHaveBeenCalledTimes(1)
  })

  it('ignores an obsolete page failure after opening personal settings', async () => {
    const oldRequest = deferred<typeof forbidden>()
    api.admin.users.list.mockReturnValueOnce(oldRequest.promise)
    const store = createStore()
    store.user = administrator
    const loading = store.loadUsers()
    await store.navigate('settings')
    oldRequest.resolve(forbidden)
    await loading
    expect(store.user?.id).toBe(administrator.id)
    expect(store.page).toBe('settings')
    expect(store.error).toBe('')
    expect(store.loading).toBe(false)
    expect(api.iam.auth.currentUser).not.toHaveBeenCalled()
  })

  it('refreshes revoked roles once and clears administration data', async () => {
    api.admin.users.list.mockResolvedValueOnce(forbidden)
    const store = createStore()
    store.user = administrator
    store.roles = [{ id: 'role-id', code: 'admin', name: 'Administrator', userCount: 1 }]
    await store.loadUsers()
    expect(store.user?.roles).toEqual(['user'])
    expect(store.page).toBe('settings')
    expect(store.roles).toEqual([])
    expect(store.users).toEqual([])
    expect(store.loading).toBe(false)
    expect(store.error).toBe(m.forbidden())
    expect(api.iam.auth.currentUser).toHaveBeenCalledTimes(1)
  })

  it('reports a connection failure when loading the authenticated user', async () => {
    api.iam.auth.currentUser.mockRejectedValueOnce(new Error('Connection unavailable'))
    const store = createStore()
    await store.authenticated()
    expect(store.error).toBe(m.network_error())
    expect(store.error).not.toBe(m.login_invalid())
    expect(store.saving).toBe(false)
    expect(api.iam.auth.currentUser).toHaveBeenCalledTimes(1)
  })
})
