import { apiContract } from '@h-ai/api-contract'
import { describe, expect, it } from 'vitest'
import * as contracts from '../src/index.js'

describe('spacegate-admin-contract', () => {
  it('defines app contract routes', () => {
    expect(contracts.spacegateAdminContract.app).toBeDefined()
    expect(contracts.appContract.info).toBeDefined()
    expect(apiContract.pathOf(contracts.appContract.info)).toBe('/app/info')
  })

  it('validates app info payload', () => {
    const result = contracts.AppInfoOutputDataSchema.safeParse({
      name: 'spacegate-admin-serv',
      version: '0.1.0',
      env: 'test',
      defaultLocale: 'zh-CN',
      supportedLocales: ['zh-CN', 'en-US'],
      startedAt: '2026-01-01T00:00:00.000Z',
      frontends: [...contracts.ENABLED_FRONTENDS],
    })

    expect(result.success).toBe(true)
  })

  it('validates echo input boundaries', () => {
    expect(contracts.AppEchoInputSchema.safeParse({ message: 'hello' }).success).toBe(true)
    expect(contracts.AppEchoInputSchema.safeParse({ message: '' }).success).toBe(false)
  })
})
