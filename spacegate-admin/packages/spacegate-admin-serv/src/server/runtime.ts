import type { CoreConfig, HaiResult } from '@h-ai/core'
import { mkdir } from 'node:fs/promises'
import path from 'node:path'
import process from 'node:process'
import { audit, AuditInitConfigSchema } from '@h-ai/audit'
import { cache, CacheConfigSchema } from '@h-ai/cache'
import { core } from '@h-ai/core'
import { crypto } from '@h-ai/crypto'
import { iam, IamConfigSchema } from '@h-ai/iam'
import { reach, ReachConfigSchema } from '@h-ai/reach'
import { reldb, ReldbConfigSchema } from '@h-ai/reldb'
import { ServConfigSchema } from '@h-ai/serv'
import { storage, StorageConfigSchema } from '@h-ai/storage'
import { serverM } from './messages.js'

let initialized = false
let initializing: Promise<void> | undefined
let closing: Promise<void> | undefined
let cleanupPending = false

/** 初始化失败停止启动并反序释放资源；不把功能缺失降级为成功。 */
export async function initializeModules(): Promise<void> {
  if (closing)
    await closing
  if (initialized)
    return
  initializing ??= runInitialization().finally(() => {
    initializing = undefined
  })
  return initializing
}

async function runInitialization(): Promise<void> {
  if (cleanupPending)
    await cleanupModules()
  cleanupPending = true
  try {
    requireInitialized(core.config.validate('serv', ServConfigSchema), 'serv-config')
    const db = ReldbConfigSchema.parse(core.config.getOrThrow('db'))
    const cacheConfig = CacheConfigSchema.parse(core.config.getOrThrow('cache'))
    const storageConfig = StorageConfigSchema.parse(core.config.getOrThrow('storage'))
    const iamConfig = IamConfigSchema.parse(core.config.getOrThrow('iam'))
    const auditConfig = AuditInitConfigSchema.parse(core.config.get('audit') ?? {})
    if (db.type === 'sqlite' && db.database !== ':memory:')
      await mkdir(path.dirname(db.database), { recursive: true })
    requireInitialized(await reldb.init(db), 'reldb')
    requireInitialized(await cache.init(cacheConfig), 'cache')
    requireInitialized(await crypto.init(), 'crypto')
    requireInitialized(await storage.init(storageConfig), 'storage')
    const reachConfig = core.config.get('reach')
    if (reachConfig)
      requireInitialized(await reach.init(ReachConfigSchema.parse(reachConfig)), 'reach')
    requireInitialized(await iam.init({ ...iamConfig, reach }), 'iam')
    requireInitialized(await audit.init(auditConfig), 'audit')
    await ensureRolesAndAdmin()
    initialized = true
    cleanupPending = false
    const app = core.config.getOrThrow<CoreConfig>('core')
    core.logger.info('Application initialized', { name: app.name, version: app.version, environment: app.env })
  }
  catch (error) {
    try {
      await cleanupModules()
    }
    catch (cleanupError) {
      throw new AggregateError([error, cleanupError], 'Application startup and cleanup failed')
    }
    throw error
  }
}

/** 此函数仅用于进程启动边界，业务请求继续返回 HaiResult。 */
function requireInitialized<T>(result: HaiResult<T>, moduleName: string): T {
  if (!result.success)
    throw new Error(`Failed to initialize ${moduleName}: ${result.error.message} (${result.error.code})`)
  return result.data
}

async function ensureRolesAndAdmin(): Promise<void> {
  for (const code of ['admin', 'user'] as const) {
    const existing = requireInitialized(await iam.authz.getRoleByCode(code), 'iam-role')
    if (!existing)
      requireInitialized(await iam.authz.createRole({ code, name: serverM(code), description: serverM(code === 'admin' ? 'adminDescription' : 'userDescription'), isSystem: true }), 'iam-role')
  }
  const users = requireInitialized(await iam.user.listUsers({ page: 1, pageSize: 1 }), 'iam-users')
  if (users.total > 0)
    return
  const password = process.env.HAI_ADMIN_DEFAULT_PASSWORD?.trim()
  if (!password)
    throw new Error('Set HAI_ADMIN_DEFAULT_PASSWORD before the first start to create the administrator account')
  const registered = requireInitialized(await iam.user.createUser({ username: 'admin', password, displayName: serverM('admin') }), 'iam-bootstrap')
  const adminRole = requireInitialized(await iam.authz.getRoleByCode('admin'), 'iam-role')
  if (!adminRole)
    throw new Error('Administrator role is missing')
  const assigned = await iam.authz.syncRoles(registered.user.id, [adminRole.id])
  if (!assigned.success) {
    const removed = await iam.user.deleteUser(registered.user.id)
    if (!removed.success)
      core.logger.error('Failed to clean up administrator bootstrap', { error: removed.error.code })
    requireInitialized(assigned, 'iam-bootstrap-role')
  }
  core.logger.info('Initial administrator created', { username: 'admin', passwordSource: 'HAI_ADMIN_DEFAULT_PASSWORD' })
}

/** 关闭顺序与依赖初始化顺序相反，同时覆盖启动失败路径。 */
export async function closeModules(): Promise<void> {
  if (!closing) {
    const pending = initializing
    closing = (async () => {
      if (pending)
        await pending.catch(() => undefined)
      await cleanupModules()
    })().finally(() => {
      closing = undefined
    })
  }
  return closing
}

/** 不依赖 isInitialized 推断资源是否释放；关闭失败必须保留义务供重试。 */
async function cleanupModules(): Promise<void> {
  const failures: unknown[] = []
  for (const [name, module] of [['audit', audit], ['iam', iam], ['reach', reach], ['storage', storage], ['crypto', crypto], ['cache', cache], ['reldb', reldb]] as const) {
    try {
      const result = await module.close()
      if (result && !result.success) {
        core.logger.error('Failed to close application module', { module: name, error: result.error.code })
        failures.push(result.error)
      }
    }
    catch (error) {
      failures.push(error)
    }
  }
  initialized = false
  cleanupPending = failures.length > 0
  if (failures.length > 0)
    throw new AggregateError(failures, 'Application cleanup failed; retry closeModules()')
}
