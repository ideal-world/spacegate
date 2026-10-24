import { HaiApiClientError } from '@h-ai/api-client'
import { authenticateTransportRequest, createInMemoryKeyStore, crypto, TRANSPORT_PROTOCOL } from '@h-ai/crypto'
import { afterAll, beforeAll, describe, expect, it, vi } from 'vitest'
import { initializeManagementApi, managementApi, resolveAvatarUrl } from '../src/lib/api/api-client.js'

const runtimeInfo = {
  name: 'Configured workspace',
  version: '9.8.7',
  env: 'test',
  defaultLocale: 'zh-CN',
  supportedLocales: ['zh-CN', 'en-US'],
  startedAt: '2026-01-01T00:00:00.000Z',
  frontends: ['web'],
}

/** 使用真实 crypto 公共入口模拟对端，覆盖握手及双向加解密。 */
function createServerFetch() {
  const key = crypto.asymmetric.generateKeyPair()
  if (!key.success)
    throw new Error(key.error.message)
  const created = crypto.transport.createServer({ serverKeyRing: { activeKeyId: 'test-v1', keys: { 'test-v1': key.data } }, keyStore: createInMemoryKeyStore() })
  if (!created.success)
    throw new Error(created.error.message)
  const server = created.data
  const requests: Request[] = []
  const messages: string[] = []
  const fetchFn: typeof fetch = async (input, init) => {
    const request = new Request(input, init)
    requests.push(request.clone())
    const url = new URL(request.url)
    if (url.pathname.endsWith('/_hai/key-exchange')) {
      const body = await request.json() as { clientPublicKey: string }
      const clientId = await server.registerClientKey(body.clientPublicKey)
      return Response.json({ serverKeyId: server.getServerKeyId(), serverPublicKey: server.getServerPublicKey(), clientId })
    }
    const clientId = request.headers.get(TRANSPORT_PROTOCOL.CLIENT_ID_HEADER)
    if (!clientId)
      return Response.json({ success: false }, { status: 400 })
    const verified = await authenticateTransportRequest(server, request)
    if (!verified.success)
      throw new Error(verified.error.message)
    let data: unknown = runtimeInfo
    if (url.pathname.endsWith('/app/echo')) {
      expect(request.headers.get(TRANSPORT_PROTOCOL.ENCRYPTED_HEADER)).toBe(TRANSPORT_PROTOCOL.ENCRYPTED_HEADER_VALUE)
      const payload = JSON.parse(verified.data.plaintext) as { message: string }
      messages.push(payload.message)
      data = { message: payload.message, echoedAt: '2026-01-01T00:00:00.000Z', requestId: 'test-request' }
    }
    const response = await server.encryptResponse(verified.data.context, JSON.stringify({ success: true, data }), 200)
    if (!response.success)
      throw new Error(response.error.message)
    return Response.json(response.data, { headers: { [TRANSPORT_PROTOCOL.ENCRYPTED_HEADER]: TRANSPORT_PROTOCOL.ENCRYPTED_HEADER_VALUE } })
  }
  return { fetchFn, requests, messages }
}

describe('shared API transport', () => {
  beforeAll(async () => {
    expect((await crypto.init()).success).toBe(true)
  })
  afterAll(async () => {
    await managementApi.close()
    await crypto.close()
  })

  it('decrypts configured core metadata and includes refresh cookies', async () => {
    const server = createServerFetch()
    expect((await initializeManagementApi({ fetchFn: server.fetchFn, baseUrl: 'https://api.example.com', clientName: 'configured-client' })).success).toBe(true)
    const info = await managementApi.app.info()
    expect(info.success).toBe(true)
    if (info.success)
      expect(info.data).toEqual(runtimeInfo)
    expect(server.requests.every(request => request.credentials === 'include')).toBe(true)
    expect(server.requests.at(-1)?.headers.get('x-request-client')).toBe('configured-client')
  })

  it.each([false, true])('uses the injected API origin when native configuration is required: %s', async (requireApiBase) => {
    Object.assign(globalThis, { __HAI_PUBLIC_CONFIG__: { apiBase: 'https://runtime.example.com' } })
    const server = createServerFetch()
    try {
      expect((await initializeManagementApi({ fetchFn: server.fetchFn, requireApiBase })).success).toBe(true)
      expect((await managementApi.app.info()).success).toBe(true)
      expect(server.requests.every(request => request.url.startsWith('https://runtime.example.com/api/v1/'))).toBe(true)
    }
    finally {
      Reflect.deleteProperty(globalThis, '__HAI_PUBLIC_CONFIG__')
    }
  })

  it('encrypts business input and preserves the native token transport header', async () => {
    const server = createServerFetch()
    const init = await initializeManagementApi({ fetchFn: server.fetchFn, baseUrl: 'https://api.example.com/api/v1', headers: { 'X-HAI-Token-Transport': 'body' } })
    expect(init.success).toBe(true)
    try {
      const result = await managementApi.app.echo({ message: 'private message' })
      expect(result.success).toBe(true)
      if (result.success)
        expect(result.data.message).toBe('private message')
      expect(server.messages).toEqual(['private message'])
      expect(server.requests.at(-1)?.headers.get('X-HAI-Token-Transport')).toBe('body')
    }
    finally {
      await managementApi.close()
    }
  })

  it('resolves avatars only under the configured API origin', async () => {
    const server = createServerFetch()
    expect((await initializeManagementApi({ fetchFn: server.fetchFn, baseUrl: 'https://trusted.example.com/api/v1' })).success).toBe(true)
    try {
      const path = '/api/v1/assets/avatars/user-id/asset-id.png'
      expect(resolveAvatarUrl(path)).toBe(`https://trusted.example.com${path}`)
      expect(resolveAvatarUrl(`https://host-injection.example.com${path}`)).toBe(`https://trusted.example.com${path}`)
      expect(resolveAvatarUrl('https://host-injection.example.com/pixel.png')).toBeUndefined()
      expect(resolveAvatarUrl('/api/v1/assets/avatars/../secrets.txt')).toBeUndefined()
      expect(resolveAvatarUrl('data:image/svg+xml,<svg/>')).toBeUndefined()
    }
    finally {
      await managementApi.close()
    }
  })

  it('requires an explicit API origin for a native build', async () => {
    vi.stubEnv('PUBLIC_API_BASE', '')
    try {
      const result = await initializeManagementApi({ requireApiBase: true })
      expect(result.success).toBe(false)
      if (!result.success)
        expect(result.error.code).toBe(HaiApiClientError.CONFIG_ERROR.code)
    }
    finally {
      vi.unstubAllEnvs()
    }
  })

  it('preserves a configured API prefix for transport and avatars', async () => {
    const server = createServerFetch()
    expect((await initializeManagementApi({ fetchFn: server.fetchFn, baseUrl: 'https://trusted.example.com/custom-api/' })).success).toBe(true)
    try {
      expect((await managementApi.app.info()).success).toBe(true)
      expect(server.requests.every(request => request.url.startsWith('https://trusted.example.com/custom-api/'))).toBe(true)
      const avatar = '/custom-api/assets/avatars/user-id/asset-id.png'
      expect(resolveAvatarUrl(avatar)).toBe(`https://trusted.example.com${avatar}`)
      expect(resolveAvatarUrl('/api/v1/assets/avatars/user-id/asset-id.png')).toBeUndefined()
    }
    finally {
      await managementApi.close()
    }
  })
})
