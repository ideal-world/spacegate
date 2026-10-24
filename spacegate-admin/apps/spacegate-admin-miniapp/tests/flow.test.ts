import { afterEach, beforeEach, describe, expect, it } from 'vitest'
import {
  ConflictError,
  configureApi,
  hasSession,
  login,
  logout,
  resetApiStateForTests,
  spacegate,
} from '../miniprogram/lib/api'
import { clearCalls, fail, installWx, loginPayload, ok, recordedCalls } from './helpers/wx-mock'

const BASE = 'http://test.local/api/miniapp/v1'

interface ServerState {
  version: number
  gateway: Record<string, unknown> | null
}

let restore: (() => void) | undefined
let state: ServerState

beforeEach(() => {
  resetApiStateForTests()
  clearCalls()
  state = { version: 1, gateway: { listeners: [] } }
})

afterEach(() => {
  restore?.()
  restore = undefined
})

describe('miniapp admin flow', () => {
  it('logs in, reads overview, resolves a 409, writes, and logs out', async () => {
    restore = installWx((call) => {
      if (call.url.endsWith('/auth/login'))
        return ok(loginPayload())
      if (call.url.endsWith('/auth/logout'))
        return ok({ success: true })
      if (call.url.endsWith('/spacegate/config/names'))
        return ok({ items: ['gw-a'], version: state.version })
      if (call.url.endsWith('/spacegate/gateway/get'))
        return ok({ gateway: state.gateway, version: state.version })
      if (call.url.endsWith('/spacegate/gateway/upsert')) {
        const input = call.data as { gateway: Record<string, unknown>, version: number }
        if (input.version !== state.version)
          return fail(409, `serverVersion=${state.version}: stale version`)
        state.gateway = input.gateway
        state.version++
        return ok({ version: state.version })
      }
      if (call.url.endsWith('/spacegate/route/list'))
        return ok({ items: [], version: state.version })
      if (call.url.endsWith('/spacegate/plugin/list'))
        return ok({ items: [], version: state.version })
      if (call.url.endsWith('/spacegate/discovery/instances'))
        return ok({ items: ['node-1'], version: state.version })
      if (call.url.endsWith('/spacegate/discovery/health'))
        return ok({ health: { 'node-1': true }, version: state.version })
      return fail(404, 'not found')
    })
    configureApi({ baseUrl: BASE })

    // 1. 登录
    await login('admin', 'secret')
    expect(hasSession()).toBe(true)

    // 2. 概览读取网关、路由、插件与实例状态
    const names = await spacegate.config.names()
    expect(names.items).toEqual(['gw-a'])
    const gateway = await spacegate.gateway.get('gw-a')
    expect(gateway.version).toBe(1)
    const routes = await spacegate.route.list({ gateway: 'gw-a' })
    expect(routes.items).toEqual([])
    const plugins = await spacegate.plugin.list({})
    expect(plugins.items).toEqual([])
    const instances = await spacegate.discovery.instances()
    const health = await spacegate.discovery.health()
    expect(instances.items).toEqual(['node-1'])
    expect(health.health['node-1']).toBe(true)

    // 3. 另一个管理员推进版本，当前写入应得到 409 且携带最新版本
    state.version = 2
    const conflict = await spacegate.gateway.upsert({ name: 'gw-a', gateway: { listeners: [{ port: 80 }] }, version: 1, create: false }).catch((error: unknown) => error)
    expect(conflict).toBeInstanceOf(ConflictError)
    if (conflict instanceof ConflictError)
      expect(conflict.serverVersion).toBe(2)

    // 4. 冲突后重新读取，再用新版本写入
    const reloaded = await spacegate.gateway.get('gw-a')
    expect(reloaded.version).toBe(2)
    const written = await spacegate.gateway.upsert({
      name: 'gw-a',
      gateway: { listeners: [{ port: 443 }] },
      version: reloaded.version,
      create: false,
    })
    expect(written.version).toBe(3)
    expect(state.gateway).toEqual({ listeners: [{ port: 443 }] })

    // 5. 退出登录清理内存
    await logout()
    expect(hasSession()).toBe(false)
    const calls = recordedCalls()
    expect(calls.filter(call => call.url.endsWith('/auth/logout'))).toHaveLength(1)
    expect(calls.some(call => call.url.endsWith('/auth/refresh'))).toBe(false)
  })
})
