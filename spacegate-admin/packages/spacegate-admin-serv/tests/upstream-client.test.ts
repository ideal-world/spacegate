import type { SpacegateUpstreamConfig } from '../src/server/upstream/spacegate-upstream-client.js'
import { HaiCommonError } from '@h-ai/core'
import { describe, expect, it } from 'vitest'
import { createSpacegateUpstreamClient } from '../src/server/upstream/spacegate-upstream-client.js'

interface FetchCall {
  url: string
  method: string
  headers: Headers
  body?: string
}

const BASE_CONFIG: SpacegateUpstreamConfig = {
  adminBaseUrl: 'http://admin.test',
  adminAk: 'ak',
  adminSk: 'sk',
  aiGatewayBaseUrl: 'http://ai.test',
  allowInsecure: false,
  requestTimeoutMs: 1_000,
  tokenTtlSeconds: 300,
}

/** 记录请求并按 URL 返回预设响应；不访问网络。 */
function mockFetch(handler: (call: FetchCall, index: number) => Response) {
  const calls: FetchCall[] = []
  const fetchFn = (async (input: RequestInfo | URL, init?: RequestInit): Promise<Response> => {
    const call: FetchCall = {
      url: String(input),
      method: init?.method ?? 'GET',
      headers: new Headers(init?.headers),
      ...(typeof init?.body === 'string' ? { body: init.body } : {}),
    }
    calls.push(call)
    return handler(call, calls.length - 1)
  }) as typeof fetch
  return { calls, fetchFn }
}

/** 构造带 jwt Set-Cookie 的登录响应。 */
function loginResponse(token: string): Response {
  return new Response(null, {
    status: 200,
    headers: [['set-cookie', `jwt=${token}; path=/; HttpOnly; Max-Age=3600`]],
  })
}

/** 构造 JSON 响应，可附加 X-Server-Version。 */
function jsonResponse(body: unknown, status = 200, headers: Record<string, string> = {}): Response {
  return new Response(JSON.stringify(body), {
    status,
    headers: { 'content-type': 'application/json', ...headers },
  })
}

/** 构造纯文本错误响应。 */
function textResponse(status: number, body: string, headers: Record<string, string> = {}): Response {
  return new Response(body, {
    status,
    headers: { 'content-type': 'text/plain; charset=utf-8', ...headers },
  })
}

function createClient(fetchFn: typeof fetch, overrides: Partial<SpacegateUpstreamConfig> = {}) {
  return createSpacegateUpstreamClient({ config: { ...BASE_CONFIG, ...overrides }, fetchFn })
}

function isLogin(call: FetchCall): boolean {
  return call.url.endsWith('/auth/login')
}

function isNames(call: FetchCall): boolean {
  return call.url.endsWith('/config/names')
}

describe('spacegate upstream client', () => {
  it('logs in on the first read and sends bearer plus client version', async () => {
    const { calls, fetchFn } = mockFetch((call) => {
      if (isLogin(call))
        return loginResponse('token-1')
      if (isNames(call))
        return jsonResponse(['gw-a'], 200, { 'x-server-version': '7' })
      return textResponse(404, 'unexpected')
    })
    const client = createClient(fetchFn)

    const result = await client.configNames()

    expect(result.success).toBe(true)
    if (result.success) {
      expect(result.data.items).toEqual(['gw-a'])
      expect(result.data.version).toBe(7)
    }
    expect(calls).toHaveLength(2)
    expect(calls[0]?.method).toBe('POST')
    expect(calls[0]?.url).toBe('http://admin.test/auth/login')
    expect(calls[0]?.body).toBe(JSON.stringify({ ak: 'ak', sk: 'sk' }))
    expect(calls[1]?.headers.get('authorization')).toBe('Bearer token-1')
    expect(calls[1]?.headers.get('x-client-version')).toBe('0')
  })

  it('refetches the version from GET /config/names after a successful write', async () => {
    const { calls, fetchFn } = mockFetch((call) => {
      if (isLogin(call))
        return loginResponse('token-1')
      if (call.method === 'PUT' && call.url.endsWith('/config/item/gw-a/gateway'))
        return new Response(null, { status: 200 })
      if (isNames(call))
        return jsonResponse(['gw-a'], 200, { 'x-server-version': '8' })
      return textResponse(404, 'unexpected')
    })
    const client = createClient(fetchFn)

    const result = await client.gatewayUpsert({
      name: 'gw-a',
      gateway: { name: 'gw-a', listeners: [] },
      version: 7,
      create: false,
    })

    expect(result.success).toBe(true)
    if (result.success)
      expect(result.data.version).toBe(8)
    expect(calls.map(call => `${call.method} ${new URL(call.url).pathname}`)).toEqual([
      'POST /auth/login',
      'PUT /config/item/gw-a/gateway',
      'GET /config/names',
    ])
    expect(calls[1]?.headers.get('x-client-version')).toBe('7')
    expect(JSON.parse(calls[1]?.body ?? '{}')).toEqual({ name: 'gw-a', listeners: [] })
  })

  it('maps 409 to CONFLICT with serverVersion and never retries the write', async () => {
    const { calls, fetchFn } = mockFetch((call) => {
      if (isLogin(call))
        return loginResponse('token-1')
      if (call.method === 'PUT')
        return textResponse(409, 'stale config document', { 'x-server-version': '9' })
      return textResponse(404, 'unexpected')
    })
    const client = createClient(fetchFn)

    const result = await client.gatewayUpsert({
      name: 'gw-a',
      gateway: { name: 'gw-a' },
      version: 7,
      create: false,
    })

    expect(result.success).toBe(false)
    if (!result.success) {
      expect(result.error.code).toBe(HaiCommonError.CONFLICT.code)
      expect(result.error.message).toContain('serverVersion=9')
      expect(result.error.message).toContain('stale config document')
    }
    expect(calls.filter(call => call.method === 'PUT')).toHaveLength(1)
    expect(calls.filter(isNames)).toHaveLength(0)
  })

  it('re-logins once after a 401 and retries the request', async () => {
    let loginCount = 0
    let namesCount = 0
    const { calls, fetchFn } = mockFetch((call) => {
      if (isLogin(call)) {
        loginCount++
        return loginResponse(`token-${loginCount}`)
      }
      if (isNames(call)) {
        namesCount++
        if (namesCount === 1)
          return textResponse(401, 'expired')
        return jsonResponse(['gw-a'], 200, { 'x-server-version': '3' })
      }
      return textResponse(404, 'unexpected')
    })
    const client = createClient(fetchFn)

    const result = await client.configNames()

    expect(result.success).toBe(true)
    if (result.success)
      expect(result.data.version).toBe(3)
    expect(loginCount).toBe(2)
    expect(namesCount).toBe(2)
    expect(calls.filter(isLogin)).toHaveLength(2)
    expect(calls.at(-1)?.headers.get('authorization')).toBe('Bearer token-2')
  })

  it('returns UNAUTHORIZED after a second consecutive 401', async () => {
    let loginCount = 0
    const { calls, fetchFn } = mockFetch((call) => {
      if (isLogin(call)) {
        loginCount++
        return loginResponse(`token-${loginCount}`)
      }
      if (isNames(call))
        return textResponse(401, 'still unauthorized')
      return textResponse(404, 'unexpected')
    })
    const client = createClient(fetchFn)

    const result = await client.configNames()

    expect(result.success).toBe(false)
    if (!result.success)
      expect(result.error.code).toBe(HaiCommonError.UNAUTHORIZED.code)
    expect(loginCount).toBe(2)
    expect(calls.filter(isNames)).toHaveLength(2)
  })

  it('preserves textual upstream errors', async () => {
    const { fetchFn } = mockFetch((call) => {
      if (isLogin(call))
        return loginResponse('token-1')
      if (isNames(call))
        return textResponse(500, 'backend exploded: disk full')
      return textResponse(404, 'unexpected')
    })
    const client = createClient(fetchFn)

    const result = await client.configNames()

    expect(result.success).toBe(false)
    if (!result.success) {
      expect(result.error.code).toBe(HaiCommonError.API_ERROR.code)
      expect(result.error.message).toBe('backend exploded: disk full')
    }
  })

  it('encodes plugin ids and a Chinese display name in the upsert query', async () => {
    const { calls, fetchFn } = mockFetch((call) => {
      if (isLogin(call))
        return loginResponse('token-1')
      if (call.method === 'PUT' && call.url.includes('/config/plugin'))
        return new Response(null, { status: 200 })
      if (isNames(call))
        return jsonResponse(['gw-a'], 200, { 'x-server-version': '12' })
      return textResponse(404, 'unexpected')
    })
    const client = createClient(fetchFn)
    const spec = { cache_url: 'redis://redis:6379', ttl_seconds: 30 }

    await client.pluginUpsert({
      plugin: {
        id: { code: 'hai-auth', kind: 'named', name: 'auth-a1' },
        displayName: '  生产 鉴权  ',
        spec,
      },
      version: 11,
      create: false,
    })
    await client.pluginUpsert({
      plugin: {
        id: { code: 'wasm', kind: 'anon', uid: 'uid-1' },
        displayName: '   ',
        spec: {},
      },
      version: 12,
      create: true,
    })

    const upserts = calls.filter(call => call.url.includes('/config/plugin'))
    expect(upserts).toHaveLength(2)

    const named = new URL(upserts[0]?.url ?? '')
    expect(named.pathname).toBe('/config/plugin')
    expect(named.searchParams.get('code')).toBe('hai-auth')
    expect(named.searchParams.get('kind')).toBe('named')
    expect(named.searchParams.get('name')).toBe('auth-a1')
    expect(named.searchParams.has('uid')).toBe(false)
    expect(named.searchParams.get('display_name')).toBe('生产 鉴权')
    expect(JSON.parse(upserts[0]?.body ?? '{}')).toEqual(spec)

    const anon = new URL(upserts[1]?.url ?? '')
    expect(anon.searchParams.get('code')).toBe('wasm')
    expect(anon.searchParams.get('kind')).toBe('anon')
    expect(anon.searchParams.get('uid')).toBe('uid-1')
    expect(anon.searchParams.has('name')).toBe(false)
    expect(anon.searchParams.has('display_name')).toBe(false)
    expect(upserts[1]?.method).toBe('POST')
    expect(calls.filter(isLogin)).toHaveLength(1)
  })

  it('renames a route with the full route body and the new route_name', async () => {
    const { calls, fetchFn } = mockFetch((call) => {
      if (isLogin(call))
        return loginResponse('token-1')
      if (call.method === 'POST' && call.url.endsWith('/rename'))
        return new Response(null, { status: 200 })
      if (isNames(call))
        return jsonResponse(['gw-a'], 200, { 'x-server-version': '5' })
      return textResponse(404, 'unexpected')
    })
    const client = createClient(fetchFn)
    const routeConfig = {
      kind: 'http',
      route_name: 'old-route',
      hostnames: ['api.example.com'],
      backends: [{ kind: 'Host', host: '10.0.0.1' }],
    }

    const result = await client.routeRename({
      gateway: 'gw-a',
      route: 'old-route',
      newName: 'new-route',
      config: routeConfig,
      version: 4,
    })

    expect(result.success).toBe(true)
    if (result.success)
      expect(result.data.version).toBe(5)
    const rename = calls.find(call => call.url.endsWith('/rename'))
    expect(rename?.method).toBe('POST')
    expect(new URL(rename?.url ?? '').pathname).toBe('/config/item/gw-a/route/item/old-route/rename')
    expect(rename?.headers.get('x-client-version')).toBe('4')
    expect(JSON.parse(rename?.body ?? '{}')).toEqual({ ...routeConfig, route_name: 'new-route' })
  })

  it('calls the configured AI Gateway admin API without a Rust JWT', async () => {
    const { calls, fetchFn } = mockFetch((call) => {
      const pathname = new URL(call.url).pathname
      if (pathname === '/v1/admin/plugins/ai-gateway-queue/schema')
        return jsonResponse({ type: 'object' })
      if (pathname === '/v1/admin/plugins/custom-plugin/readme')
        return new Response('# Custom readme', { status: 200, headers: { 'content-type': 'text/markdown' } })
      if (pathname === '/v1/admin/tenant-rate-limits/resolve')
        return jsonResponse({ tenant: 't1', policy: 'queue', rps: 10 })
      if (pathname === '/v1/admin/tenant-rate-limits' && call.method === 'GET')
        return jsonResponse([{ tenant: 't1', rps: 10 }])
      if (pathname === '/v1/admin/tenant-rate-limits' && call.method === 'PUT')
        return jsonResponse({ tenant: 't1', rps: 20 })
      if (pathname === '/v1/admin/tenant-rate-limits' && call.method === 'DELETE')
        return jsonResponse({ deleted: 1 })
      return textResponse(404, 'unexpected')
    })
    const client = createClient(fetchFn)

    const schema = await client.aiQueueSchema({})
    const readme = await client.aiQueueReadme({ plugin: 'custom-plugin' })
    const rules = await client.aiQueueRules({ filters: { tenant: 't1', model: 'm 1' } })
    const upserted = await client.aiQueueUpsert({ rule: { tenant: 't1', rps: 20 } })
    const deleted = await client.aiQueueDelete({ rule: { tenant: 't1' } })
    const resolved = await client.aiQueueResolve({ tenant: 't1', model: 'm1' })

    expect(schema.success).toBe(true)
    expect(readme.success).toBe(true)
    if (readme.success)
      expect(readme.data.markdown).toBe('# Custom readme')
    expect(rules.success).toBe(true)
    if (rules.success)
      expect(rules.data.items).toEqual([{ tenant: 't1', rps: 10 }])
    expect(upserted.success).toBe(true)
    if (upserted.success)
      expect(upserted.data.rule).toEqual({ tenant: 't1', rps: 20 })
    expect(deleted.success).toBe(true)
    if (deleted.success)
      expect(deleted.data.deleted).toBe(1)
    expect(resolved.success).toBe(true)
    if (resolved.success)
      expect(resolved.data.resolution).toEqual({ tenant: 't1', policy: 'queue', rps: 10 })

    expect(calls.length).toBeGreaterThan(0)
    expect(calls.every(call => call.url.startsWith('http://ai.test/'))).toBe(true)
    expect(calls.every(call => !call.headers.has('authorization'))).toBe(true)
    expect(calls.some(call => call.url.startsWith('http://admin.test'))).toBe(false)
    const rulesCall = calls.find(call => call.url.includes('tenant-rate-limits?') && call.method === 'GET')
    expect(new URL(rulesCall?.url ?? '').searchParams.get('model')).toBe('m 1')
  })

  it('fails explicitly when credentials are missing unless allowInsecure is true', async () => {
    const strict = createClient(mockFetch(() => textResponse(500, 'should not reach upstream')).fetchFn, {
      adminAk: undefined,
      adminSk: undefined,
      allowInsecure: false,
    })
    await expect(strict.configNames()).rejects.toThrow(/allowInsecure/)

    const { calls, fetchFn } = mockFetch((call) => {
      if (isNames(call))
        return jsonResponse(['gw-a'], 200, { 'x-server-version': '2' })
      return textResponse(404, 'unexpected')
    })
    const insecure = createClient(fetchFn, {
      adminAk: undefined,
      adminSk: undefined,
      allowInsecure: true,
    })

    const result = await insecure.configNames()

    expect(result.success).toBe(true)
    if (result.success)
      expect(result.data.version).toBe(2)
    expect(calls).toHaveLength(1)
    expect(calls[0]?.headers.has('authorization')).toBe(false)
    expect(calls.some(isLogin)).toBe(false)
  })

  it('lists plugin instances by code and keeps the full-list fallback', async () => {
    const { calls, fetchFn } = mockFetch((call) => {
      if (isLogin(call))
        return loginResponse('token-1')
      if (call.url.endsWith('/config/plugins/hai-auth'))
        return jsonResponse([{ code: 'hai-auth', kind: 'named', name: 'auth-a1' }], 200, { 'x-server-version': '4' })
      if (call.url.endsWith('/config/plugin-all'))
        return jsonResponse([{ code: 'hai-auth' }, { code: 'wasm' }], 200, { 'x-server-version': '5' })
      return textResponse(404, 'unexpected')
    })
    const client = createClient(fetchFn)

    const byCode = await client.pluginList({ code: 'hai-auth' })
    const all = await client.pluginList({})

    expect(byCode.success).toBe(true)
    if (byCode.success) {
      expect(byCode.data.items).toHaveLength(1)
      expect(byCode.data.version).toBe(4)
    }
    expect(all.success).toBe(true)
    if (all.success)
      expect(all.data.items).toHaveLength(2)
    expect(calls.map(call => new URL(call.url).pathname)).toContain('/config/plugins/hai-auth')
    expect(calls.map(call => new URL(call.url).pathname)).toContain('/config/plugin-all')
  })

  it('forwards HTTP auth headers for a private Wasm schema preview', async () => {
    const { calls, fetchFn } = mockFetch((call) => {
      if (isLogin(call))
        return loginResponse('token-1')
      if (call.url.endsWith('/plugin/wasm/schema/preview'))
        return jsonResponse({ type: 'object', properties: {} })
      return textResponse(404, 'unexpected')
    })
    const client = createClient(fetchFn)

    const result = await client.pluginPreviewWasmSchema({
      imageUrl: 'https://registry.example.com/plugin:1.0.0',
      schemaPath: 'schema.json',
      httpHeaders: { 'Authorization': 'Bearer preview-secret', 'X-Tenant': 'acme' },
    })

    expect(result.success).toBe(true)
    const preview = calls.find(call => call.url.endsWith('/plugin/wasm/schema/preview'))
    expect(JSON.parse(preview?.body ?? '{}')).toEqual({
      image_url: 'https://registry.example.com/plugin:1.0.0',
      schema_path: 'schema.json',
      http_headers: { 'Authorization': 'Bearer preview-secret', 'X-Tenant': 'acme' },
    })
    expect(preview?.headers.has('x-client-version')).toBe(false)
  })

  it('rejects a login response without a jwt cookie instead of falling back to anonymous access', async () => {
    const { calls, fetchFn } = mockFetch(call => call.url.endsWith('/auth/login')
      ? new Response(null, { status: 200 })
      : textResponse(404, 'unexpected'))
    const client = createClient(fetchFn)

    const result = await client.configNames()

    expect(result.success).toBe(false)
    if (!result.success) {
      expect(result.error.code).toBe(HaiCommonError.UNAUTHORIZED.code)
      expect(result.error.message).toContain('JWT cookie')
    }
    expect(calls).toHaveLength(1)
  })
})
