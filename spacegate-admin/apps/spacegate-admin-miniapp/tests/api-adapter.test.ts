import { afterEach, beforeEach, describe, expect, it } from 'vitest'
import {
  clearSession,
  configureApi,
  ConflictError,
  DEFAULT_API_BASE_URL,
  getApiBaseUrl,
  hasSession,
  InvalidResponseError,
  login,
  logout,
  resetApiStateForTests,
  spacegate,
} from '../miniprogram/lib/api'
import { clearCalls, fail, installWx, loginPayload, ok, recordedCalls } from './helpers/wx-mock'

const BASE = 'http://test.local/api/miniapp/v1'

let restore: (() => void) | undefined

beforeEach(() => {
  resetApiStateForTests()
  clearCalls()
})

afterEach(() => {
  restore?.()
  restore = undefined
})

describe('wx api adapter', () => {
  it('logs in with the miniapp headers and injects the bearer token', async () => {
    restore = installWx((call) => {
      if (call.url.endsWith('/auth/login'))
        return ok(loginPayload())
      if (call.url.endsWith('/spacegate/config/names'))
        return ok({ items: ['gw-a'], version: 7 })
      return fail(404, 'not found')
    })
    configureApi({ baseUrl: BASE })
    await login('admin', 'secret')
    expect(hasSession()).toBe(true)
    const loginCall = recordedCalls()[0]
    expect(loginCall?.url).toBe(`${BASE}/auth/login`)
    expect(loginCall?.header['x-request-client']).toBe('spacegate-admin-miniapp')
    expect(loginCall?.header['x-hai-token-transport']).toBe('body')
    expect(loginCall?.header.authorization).toBeUndefined()

    const names = await spacegate.config.names()
    expect(names.version).toBe(7)
    const businessCall = recordedCalls()[1]
    expect(businessCall?.header.authorization).toBe('Bearer access-1')
    expect(businessCall?.method).toBe('POST')
  })

  it('defaults the base URL and never touches storage', async () => {
    const storageCalls: string[] = []
    restore = installWx(call => call.url.endsWith('/auth/login') ? ok(loginPayload()) : ok({ items: [], version: 0 }))
    const api = (globalThis as unknown as { wx: WxApi }).wx
    api.getStorageSync = (key: string) => {
      storageCalls.push(`get:${key}`)
      return ''
    }
    api.setStorageSync = (key: string) => {
      storageCalls.push(`set:${key}`)
    }
    clearSession()
    resetApiStateForTests()
    expect(getApiBaseUrl()).toBe(DEFAULT_API_BASE_URL)
    await login('admin', 'secret')
    await spacegate.config.names()
    expect(storageCalls).toEqual([])
  })

  it('refreshes once for concurrent 401 responses and retries with the new token', async () => {
    let namesCalls = 0
    let refreshCalls = 0
    restore = installWx(async (call) => {
      if (call.url.endsWith('/auth/login'))
        return ok(loginPayload('old-token', 'refresh-old'))
      if (call.url.endsWith('/auth/refresh')) {
        refreshCalls++
        await new Promise(resolve => setTimeout(resolve, 10))
        return ok({ tokens: { accessToken: 'new-token', refreshToken: 'refresh-new' } })
      }
      if (call.url.endsWith('/spacegate/config/names')) {
        namesCalls++
        return call.header.authorization === 'Bearer new-token'
          ? ok({ items: ['gw-a'], version: 1 })
          : fail(401, 'expired')
      }
      return fail(404, 'not found')
    })
    configureApi({ baseUrl: BASE })
    await login('admin', 'secret')
    const [first, second] = await Promise.all([spacegate.config.names(), spacegate.config.names()])
    expect(first.version).toBe(1)
    expect(second.version).toBe(1)
    expect(refreshCalls).toBe(1)
    expect(namesCalls).toBe(4)
  })

  it('maps 409 to a typed conflict error carrying serverVersion', async () => {
    restore = installWx((call) => {
      if (call.url.endsWith('/auth/login'))
        return ok(loginPayload())
      if (call.url.endsWith('/spacegate/gateway/upsert'))
        return fail(409, 'serverVersion=42: version conflict')
      return fail(404, 'not found')
    })
    configureApi({ baseUrl: BASE })
    await login('admin', 'secret')
    const failure = await spacegate.gateway.upsert({ name: 'gw-a', gateway: {}, version: 1, create: false }).catch((error: unknown) => error)
    expect(failure).toBeInstanceOf(ConflictError)
    if (failure instanceof ConflictError) {
      expect(failure.httpStatus).toBe(409)
      expect(failure.serverVersion).toBe(42)
    }
  })

  it('rejects malformed envelopes and clears memory only after logout', async () => {
    restore = installWx((call) => {
      if (call.url.endsWith('/auth/login'))
        return ok(loginPayload())
      if (call.url.endsWith('/auth/logout'))
        return ok({ success: true })
      return { data: { unexpected: true } }
    })
    configureApi({ baseUrl: BASE })
    await login('admin', 'secret')
    await expect(spacegate.config.names()).rejects.toBeInstanceOf(InvalidResponseError)
    expect(hasSession()).toBe(true)
    await logout()
    expect(hasSession()).toBe(false)
    const logoutCall = recordedCalls().find(call => call.url.endsWith('/auth/logout'))
    expect(logoutCall?.header.authorization).toBe('Bearer access-1')
  })
})
