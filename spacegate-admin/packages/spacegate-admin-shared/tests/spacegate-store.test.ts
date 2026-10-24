import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { HashRouter } from '../src/lib/navigation/hash-router.svelte.js'
import * as m from '../src/lib/paraglide/messages.js'
import { newGateway } from '../src/lib/spacegate/model.js'
import { ManagementStore } from '../src/lib/stores/management-store.svelte.js'

const api = vi.hoisted(() => ({
  auth: { clear: vi.fn() },
  iam: { auth: { logout: vi.fn(), currentUser: vi.fn() } },
  admin: {
    users: { list: vi.fn() },
    roles: { list: vi.fn() },
    audit: { list: vi.fn() },
  },
  spacegate: {
    config: { names: vi.fn() },
    gateway: { get: vi.fn(), upsert: vi.fn(), delete: vi.fn() },
    route: { list: vi.fn(), get: vi.fn(), upsert: vi.fn(), rename: vi.fn(), delete: vi.fn() },
    plugin: {
      list: vi.fn(),
      get: vi.fn(),
      upsert: vi.fn(),
      delete: vi.fn(),
      catalog: vi.fn(),
      attrAll: vi.fn(),
      attr: vi.fn(),
      schema: vi.fn(),
      wasmSchema: vi.fn(),
      previewWasmSchema: vi.fn(),
      refresh: vi.fn(),
    },
    discovery: {
      instances: vi.fn(),
      health: vi.fn(),
      reloadGateway: vi.fn(),
      reloadGlobal: vi.fn(),
      reloadRoute: vi.fn(),
    },
    aiQueue: {
      schema: vi.fn(),
      readme: vi.fn(),
      rules: vi.fn(),
      upsert: vi.fn(),
      delete: vi.fn(),
      resolve: vi.fn(),
    },
  },
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
const gatewayConfig = {
  name: 'gateway-a',
  parameters: { redis_url: null },
  listeners: [{ name: 'http', ip: null, port: 9000, protocol: { type: 'http' }, hostname: null }],
  plugins: [],
}
const routeList = [
  {
    name: 'route-a',
    route: {
      route_name: 'route-a',
      hostnames: null,
      priority: 0,
      plugins: [],
      rules: [{ matches: null, plugins: [], backends: [], timeout_ms: null, timeout_mode: null, balance_policy: null }],
    },
  },
]

function ok<T>(data: T) {
  return { success: true as const, data }
}

function conflict(serverVersion: number) {
  return {
    success: false as const,
    error: { code: '302:409', message: `serverVersion=${serverVersion}: version conflict`, httpStatus: 409 },
  }
}

beforeEach(() => {
  vi.resetAllMocks()
  vi.stubGlobal('location', { hash: '#/overview' })
  vi.stubGlobal('history', { replaceState: vi.fn(), pushState: vi.fn() })
  api.iam.auth.currentUser.mockResolvedValue(ok(member))
  api.spacegate.config.names.mockResolvedValue(ok({ items: ['gateway-a'], version: 7 }))
  api.spacegate.gateway.get.mockResolvedValue(ok({ gateway: gatewayConfig, version: 7 }))
  api.spacegate.route.list.mockResolvedValue(ok({ items: routeList, version: 7 }))
  api.spacegate.plugin.list.mockResolvedValue(ok({ items: [], version: 7 }))
  api.spacegate.plugin.catalog.mockResolvedValue(ok({ items: ['basic'], version: 7 }))
  api.spacegate.plugin.attrAll.mockResolvedValue(ok({ items: [], version: 7 }))
  api.spacegate.discovery.instances.mockResolvedValue(ok({ items: ['instance-a'], version: 7 }))
  api.spacegate.discovery.health.mockResolvedValue(ok({ health: { 'instance-a': true }, version: 7 }))
})

afterEach(() => vi.unstubAllGlobals())

/** 新管理页面的路由回退为 overview；测试只使用内存 hash，不访问浏览器。 */
function createStore() {
  return new ManagementStore(new HashRouter([
    '../pages/overview.svelte',
    '../pages/gateways.svelte',
    '../pages/routes.svelte',
    '../pages/plugins.svelte',
    '../pages/settings.svelte',
  ], '/overview'))
}

describe('spacegate store version discipline', () => {
  it('sends the version from the latest read and adopts the write result', async () => {
    const store = createStore()
    store.user = administrator
    await store.loadGateway('gateway-a')
    expect(store.version).toBe(7)

    api.spacegate.gateway.upsert.mockResolvedValue(ok({ version: 8 }))
    api.spacegate.gateway.get.mockResolvedValue(ok({ gateway: { ...gatewayConfig, listeners: [] }, version: 8 }))
    api.spacegate.route.list.mockResolvedValue(ok({ items: [], version: 8 }))

    const saved = await store.saveGateway(false, 'gateway-a', gatewayConfig)

    expect(saved).toBe(true)
    expect(api.spacegate.gateway.upsert).toHaveBeenCalledTimes(1)
    expect(api.spacegate.gateway.upsert).toHaveBeenCalledWith({
      name: 'gateway-a',
      gateway: expect.objectContaining({ name: 'gateway-a' }),
      version: 7,
      create: false,
    })
    expect(store.version).toBe(8)
    expect(store.notice).toBe(m.saved())
    expect(store.gateway?.listeners).toEqual([])
  })

  it('keeps the editor snapshot when a background read advances the global version', async () => {
    const store = createStore()
    store.user = administrator
    await store.loadGateway('gateway-a')
    expect(store.gatewayEditVersion).toBe(7)

    // 另一个页面刷新路由列表，把全局版本推进到 9；网关编辑器仍持有 7 的快照。
    api.spacegate.route.list.mockResolvedValue(ok({ items: routeList, version: 9 }))
    await store.loadRoutes('gateway-a')
    expect(store.version).toBe(9)
    expect(store.gatewayEditVersion).toBe(7)

    api.spacegate.gateway.upsert.mockResolvedValue(ok({ version: 10 }))
    api.spacegate.gateway.get.mockResolvedValue(ok({ gateway: gatewayConfig, version: 10 }))
    await store.saveGateway(false, 'gateway-a', gatewayConfig)

    expect(api.spacegate.gateway.upsert).toHaveBeenCalledWith(expect.objectContaining({ version: 7, create: false }))
  })

  it('keeps the edit version, does not retry and exposes an explicit conflict reload', async () => {
    const store = createStore()
    store.user = administrator
    store.page = 'gateways'
    await store.loadGateway('gateway-a')
    const readCalls = api.spacegate.gateway.get.mock.calls.length
    api.spacegate.gateway.upsert.mockResolvedValue(conflict(12))

    const saved = await store.saveGateway(false, 'gateway-a', gatewayConfig)

    expect(saved).toBe(false)
    expect(api.spacegate.gateway.upsert).toHaveBeenCalledTimes(1)
    expect(api.spacegate.gateway.upsert).toHaveBeenCalledWith(expect.objectContaining({ version: 7 }))
    expect(store.conflict).toBe(true)
    expect(store.conflictServerVersion).toBe(12)
    expect(store.version).toBe(7)
    expect(api.spacegate.gateway.get).toHaveBeenCalledTimes(readCalls)
    expect(store.notice).toBe('')
    expect(store.error).toBe(m.version_conflict({ version: '12' }))

    api.spacegate.config.names.mockResolvedValue(ok({ items: ['gateway-a'], version: 12 }))
    api.spacegate.gateway.get.mockResolvedValue(ok({ gateway: gatewayConfig, version: 12 }))
    api.spacegate.route.list.mockResolvedValue(ok({ items: routeList, version: 12 }))
    await store.reloadAfterConflict()

    expect(store.conflict).toBe(false)
    expect(store.conflictServerVersion).toBeNull()
    expect(store.version).toBe(12)
    expect(api.spacegate.gateway.get.mock.calls.length).toBeGreaterThan(readCalls)
  })

  it('creates a gateway with the read version and refreshes the selection', async () => {
    const store = createStore()
    store.user = administrator
    store.version = 5
    api.spacegate.gateway.upsert.mockResolvedValue(ok({ version: 6 }))
    api.spacegate.gateway.get.mockResolvedValue(ok({ gateway: newGateway('created-gateway'), version: 6 }))
    api.spacegate.route.list.mockResolvedValue(ok({ items: [], version: 6 }))

    const saved = await store.saveGateway(true, 'created-gateway', newGateway('created-gateway'))

    expect(saved).toBe(true)
    expect(api.spacegate.gateway.upsert).toHaveBeenCalledWith(expect.objectContaining({ create: true, version: 5, name: 'created-gateway' }))
    expect(store.selectedGateway).toBe('created-gateway')
    expect(store.version).toBe(6)
  })
})

describe('spacegate store authorization and selection', () => {
  it('does not load or navigate to management pages for non-admins', async () => {
    const store = createStore()
    store.user = member
    await store.loadOverview()
    await store.navigate('overview')
    expect(store.page).toBe('settings')
    expect(api.spacegate.config.names).not.toHaveBeenCalled()
    expect(api.spacegate.gateway.get).not.toHaveBeenCalled()
    expect(api.spacegate.gateway.upsert).not.toHaveBeenCalled()
  })

  it('selects a gateway and loads its complete configuration and routes', async () => {
    const store = createStore()
    store.user = administrator
    api.spacegate.config.names.mockResolvedValue(ok({ items: ['gateway-a', 'gateway-b'], version: 9 }))
    api.spacegate.gateway.get.mockImplementation(({ name }: { name: string }) => Promise.resolve(ok({
      gateway: { ...gatewayConfig, name },
      version: name === 'gateway-b' ? 11 : 9,
    })))
    api.spacegate.route.list.mockImplementation(({ gateway }: { gateway: string }) => Promise.resolve(ok({
      items: gateway === 'gateway-b' ? routeList : [],
      version: gateway === 'gateway-b' ? 11 : 9,
    })))
    await store.loadGateways()
    expect(store.selectedGateway).toBe('gateway-a')

    await store.selectGateway('gateway-b')

    expect(store.selectedGateway).toBe('gateway-b')
    expect(store.gateway?.name).toBe('gateway-b')
    expect(store.routes).toHaveLength(1)
    expect(store.version).toBe(11)
  })
})
