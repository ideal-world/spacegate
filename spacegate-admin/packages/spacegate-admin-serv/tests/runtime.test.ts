import { mkdtemp, rm } from 'node:fs/promises'
import os from 'node:os'
import path from 'node:path'
import { audit } from '@h-ai/audit'
import { cache } from '@h-ai/cache'
import { core } from '@h-ai/core'
import { crypto } from '@h-ai/crypto'
import { iam } from '@h-ai/iam'
import { reldb } from '@h-ai/reldb'
import { storage } from '@h-ai/storage'
import { afterEach, beforeEach, expect, it, vi } from 'vitest'
import { closeModules, initializeModules } from '../src/server/runtime.js'

let storageRoot: string

beforeEach(async () => {
  storageRoot = await mkdtemp(path.join(os.tmpdir(), 'spacegate-admin-bootstrap-'))
  vi.stubEnv('HAI_DB_DATABASE', '":memory:"')
  vi.stubEnv('HAI_STORAGE_ROOT', storageRoot)
  vi.stubEnv('HAI_IAM_PASSWORD_REQUIREUPPERCASE', 'true')
  core.init({ configDir: './config' })
})

afterEach(async () => {
  await closeModules()
  vi.unstubAllEnvs()
  await rm(storageRoot, { recursive: true, force: true })
})

it.each([
  { password: 'abc', reason: /at least 8 characters|至少为 8 个字符/ },
  { password: 'lowercase123!', reason: /uppercase letters|大写字母/ },
])('startup explains the failed password policy and permits a clean retry: $password', async ({ password, reason }) => {
  vi.stubEnv('HAI_ADMIN_DEFAULT_PASSWORD', password)
  await expect(initializeModules()).rejects.toThrow(reason)
  for (const module of [audit, iam, storage, crypto, cache, reldb])
    expect(module.isInitialized).toBe(false)

  // 失败后再次初始化，确认未保留半成品管理员或未释放的模块。
  vi.stubEnv('HAI_ADMIN_DEFAULT_PASSWORD', 'Bootstrap-retry-Admin123!')
  await initializeModules()
  const users = await iam.user.listUsers({ page: 1, pageSize: 10 })
  expect(users.success).toBe(true)
  if (users.success) {
    expect(users.data.total).toBe(1)
    expect(users.data.items[0]?.username).toBe('admin')
  }
})
