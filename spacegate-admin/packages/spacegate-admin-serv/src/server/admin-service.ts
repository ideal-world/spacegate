import type { HaiResult } from '@h-ai/core'
import type { Role, User } from '@h-ai/iam'
import type { ServConfig, ServContext } from '@h-ai/serv'
import type { AdminCreateUser, AdminRole, AdminRoleCode, AdminUpdateUser, AdminUser, AvatarUpload, ProfileUpdate } from 'spacegate-admin-contract'
import { Buffer } from 'node:buffer'
import { audit } from '@h-ai/audit'
import { cache } from '@h-ai/cache'
import { core, err, HaiCommonError, ok } from '@h-ai/core'
import { iam } from '@h-ai/iam'
import { storage } from '@h-ai/storage'
import { SUPPORTED_LOCALES } from 'spacegate-admin-contract'
import { serverM } from './messages.js'

const ROLE_CODES = new Set<AdminRoleCode>(['admin', 'user'])
// 品牌主题 sg-soft / sg-night；保留 light / dark 以兼容历史用户元数据。
const SUPPORTED_THEMES = ['sg-soft', 'sg-night', 'light', 'dark'] as readonly string[]
let adminMutationActive = false

/** 管理写操作串行化，锁内重新鉴权，防止两个管理员同时互相删除或降级。 */
export async function withAdminMutation<T>(context: ServContext, work: () => Promise<HaiResult<T>>): Promise<HaiResult<T>> {
  // mini 的管理写入仅支持单写实例；进程内保护无租期，慢请求不会在 TTL 到期后被并发穿透。
  if (adminMutationActive)
    return err(HaiCommonError.CONFLICT, serverM('busy', { locale: context.locale }))
  adminMutationActive = true
  let owner = ''
  const key = 'hai:app:admin-mutation'
  let acquired = false
  let leaseLost = false
  let renewalPending = false
  let renewalTimer: ReturnType<typeof setInterval> | undefined
  try {
    const locked = await cache.lock.acquire(key, { ttl: 300 })
    if (!locked.success)
      return locked
    if (!locked.data)
      return err(HaiCommonError.CONFLICT, serverM('busy', { locale: context.locale }))
    owner = locked.data
    acquired = true
    renewalTimer = setInterval(() => {
      if (renewalPending || leaseLost)
        return
      renewalPending = true
      void cache.lock.extend(key, 300, owner).then((renewed) => {
        if (!renewed.success || !renewed.data) {
          leaseLost = true
          core.logger.error('Admin mutation lease renewal failed', { owner, error: renewed.success ? 'lease_not_owned' : renewed.error.code })
        }
      }).catch(() => {
        leaseLost = true
        core.logger.error('Admin mutation lease renewal failed', { owner, error: 'unexpected_renewal_failure' })
      }).finally(() => {
        renewalPending = false
      })
    }, 100_000)
    renewalTimer.unref()
    const actor = await getAdminUser(context.session?.userId ?? '')
    if (!actor.success)
      return actor
    if (!actor.data.enabled || !actor.data.roles.includes('admin'))
      return err(HaiCommonError.FORBIDDEN, serverM('selfProtection', { locale: context.locale }))
    if (leaseLost)
      return err(HaiCommonError.CONFLICT, serverM('busy', { locale: context.locale }))
    return await work()
  }
  finally {
    if (renewalTimer)
      clearInterval(renewalTimer)
    try {
      if (acquired) {
        const released = await cache.lock.release(key, owner)
        if (!released.success || !released.data)
          core.logger.error('Failed to release admin mutation lock', { owner, error: released.success ? 'lease_not_owned' : released.error.code })
      }
    }
    finally {
      adminMutationActive = false
    }
  }
}

/** 用户与角色读取失败直接透传，不能把角色查询失败当作普通用户。 */
export async function getAdminUser(id: string): Promise<HaiResult<AdminUser>> {
  const user = await iam.user.getUser(id)
  if (!user.success)
    return user
  if (!user.data)
    return err(HaiCommonError.NOT_FOUND, serverM('notFound'))
  const roles = await iam.authz.getUserRoles(id)
  if (!roles.success)
    return roles
  return ok(toAdminUser(user.data, roles.data))
}

export function toAdminUser(user: User, roles: Role[]): AdminUser {
  return {
    id: user.id,
    username: user.username,
    email: user.email,
    displayName: user.displayName,
    avatarUrl: user.avatarUrl,
    enabled: user.enabled,
    roles: roles.map(role => role.code).filter(isRoleCode),
    createdAt: user.createdAt.toISOString(),
    updatedAt: user.updatedAt.toISOString(),
  }
}

export function isRoleCode(code: string): code is AdminRoleCode {
  return ROLE_CODES.has(code as AdminRoleCode)
}

export async function createAdminUser(input: AdminCreateUser, context: ServContext): Promise<HaiResult<AdminUser>> {
  const role = await iam.authz.getRoleByCode(input.role)
  if (!role.success)
    return role
  if (!role.data)
    return err(HaiCommonError.NOT_FOUND, serverM('notFound', { locale: context.locale }))
  const registered = await iam.user.createUser(input)
  if (!registered.success)
    return registered
  const userId = registered.data.user.id
  const assigned = await iam.authz.syncRoles(userId, [role.data.id])
  if (!assigned.success) {
    await rollbackCreatedUser(userId)
    return assigned
  }
  if (!input.enabled) {
    const disabled = await iam.user.updateUser(userId, { enabled: false })
    if (!disabled.success) {
      await rollbackCreatedUser(userId)
      return disabled
    }
  }
  await recordAudit(context, 'create', 'users', userId, { role: input.role, enabled: input.enabled })
  return getAdminUser(userId)
}

async function rollbackCreatedUser(id: string): Promise<void> {
  const removed = await iam.user.deleteUser(id)
  if (!removed.success)
    core.logger.error('Failed to roll back user creation', { userId: id, error: removed.error.code })
}

export async function updateAdminUser(input: AdminUpdateUser, context: ServContext): Promise<HaiResult<AdminUser>> {
  if (input.id === context.session?.userId && (input.enabled === false || input.role === 'user'))
    return err(HaiCommonError.FORBIDDEN, serverM('selfProtection', { locale: context.locale }))
  const existing = await iam.user.getUser(input.id)
  if (!existing.success)
    return existing
  if (!existing.data)
    return err(HaiCommonError.NOT_FOUND, serverM('notFound', { locale: context.locale }))
  const role = input.role ? await iam.authz.getRoleByCode(input.role) : undefined
  if (role && !role.success)
    return role
  if (role?.success && !role.data)
    return err(HaiCommonError.NOT_FOUND, serverM('notFound', { locale: context.locale }))
  const { id, role: _role, ...profile } = input
  const previousRoles = await iam.authz.getUserRoles(id)
  if (!previousRoles.success)
    return previousRoles
  if (role?.success && role.data) {
    const assigned = await iam.authz.syncRoles(id, [role.data.id])
    if (!assigned.success)
      return assigned
  }
  const updated = await iam.user.updateUser(id, profile)
  if (!updated.success) {
    // 角色 ID 可精确还原，避免 undefined 资料字段被 updateUser 忽略后无法补偿。
    if (role?.success && role.data) {
      const restored = await iam.authz.syncRoles(id, previousRoles.data.map(item => item.id))
      if (!restored.success)
        core.logger.error('Failed to restore roles after user update failure', { userId: id, error: restored.error.code })
    }
    return updated
  }
  await recordAudit(context, 'update', 'users', id, { fields: Object.keys(profile), role: input.role })
  return getAdminUser(id)
}

export async function getAdminRoles(): Promise<HaiResult<{ items: AdminRole[] }>> {
  const roles = await iam.authz.getAllRoles({ page: 1, pageSize: 100 })
  if (!roles.success)
    return roles
  const items = roles.data.items.filter(role => isRoleCode(role.code))
  const counts = await iam.authz.getRoleUserCounts(items.map(role => role.id))
  if (!counts.success)
    return counts
  return ok({ items: items.map(role => ({
    id: role.id,
    code: role.code as AdminRoleCode,
    name: role.name,
    description: role.description,
    userCount: counts.data.get(role.id) ?? 0,
  })) })
}

export async function updateProfile(input: ProfileUpdate, context: ServContext): Promise<HaiResult<AdminUser>> {
  const token = context.accessToken ?? ''
  const current = await iam.user.getCurrentUser(token)
  if (!current.success)
    return current
  if ((input.locale && !(SUPPORTED_LOCALES as readonly string[]).includes(input.locale)) || (input.theme && !SUPPORTED_THEMES.includes(input.theme)))
    return err(HaiCommonError.VALIDATION_ERROR, serverM('unsupportedPreference', { locale: context.locale }))
  const metadata = { ...current.data.metadata }
  if (input.locale)
    metadata.locale = input.locale
  if (input.theme)
    metadata.theme = input.theme
  const updated = await iam.user.updateCurrentUser(token, { displayName: input.displayName, email: input.email, metadata })
  if (!updated.success)
    return updated
  await recordAudit(context, 'update', 'profile', current.data.id, { fields: Object.keys(input) })
  return getAdminUser(current.data.id)
}

/** 上传路径由服务端生成，禁止客户端写入任意 storage key。 */
export async function uploadAvatar(userId: string, input: AvatarUpload, context: ServContext): Promise<HaiResult<{ avatarUrl: string }>> {
  const current = await iam.user.getUser(userId)
  if (!current.success)
    return current
  if (!current.data)
    return err(HaiCommonError.NOT_FOUND, serverM('notFound', { locale: context.locale }))
  const bytes = Buffer.from(input.base64, 'base64')
  if (bytes.length === 0 || bytes.length > 512 * 1024 || bytes.toString('base64') !== input.base64 || !matchesImage(bytes, input.contentType))
    return err(HaiCommonError.VALIDATION_ERROR, serverM('invalidAvatar', { locale: context.locale }))
  const extension = { 'image/png': 'png', 'image/jpeg': 'jpg', 'image/webp': 'webp' }[input.contentType]
  const key = `avatars/${userId}/${core.id.uuid()}.${extension}`
  const stored = await storage.file.put(key, bytes, { contentType: input.contentType })
  if (!stored.success)
    return stored
  const apiPrefix = core.config.getOrThrow<ServConfig>('serv').http.apiPrefix
  // 只保存生成资源路径，公开 Origin 由客户端已配置的 API 地址决定，禁止信任 Host。
  const avatarUrl = `${apiPrefix}/assets/${key}`
  const updated = await iam.user.updateUser(userId, { avatarUrl })
  if (!updated.success) {
    const cleaned = await storage.file.delete(key)
    if (!cleaned.success)
      core.logger.warn('Failed to clean up rejected avatar', { error: cleaned.error.code })
    return updated
  }
  // 更新成功后删除旧头像；只清理此用户且符合服务端生成规则的资源。
  const oldKey = current.data.avatarUrl?.split('/assets/')[1]
  if (oldKey && oldKey.startsWith(`avatars/${userId}/`) && /^avatars\/[\w-]+\/[\w-]+\.(?:png|jpg|webp)$/.test(oldKey)) {
    const cleaned = await storage.file.delete(oldKey)
    if (!cleaned.success)
      core.logger.warn('Failed to clean up previous avatar', { error: cleaned.error.code })
  }
  await recordAudit(context, 'update', 'avatars', userId)
  return ok({ avatarUrl })
}

function matchesImage(bytes: Buffer, contentType: AvatarUpload['contentType']): boolean {
  if (contentType === 'image/png') {
    if (bytes.length < 33 || bytes.subarray(0, 8).toString('hex') !== '89504e470d0a1a0a' || bytes.toString('ascii', 12, 16) !== 'IHDR')
      return false
    const width = bytes.readUInt32BE(16)
    const height = bytes.readUInt32BE(20)
    return width > 0 && height > 0 && width <= 4096 && height <= 4096
  }
  if (contentType === 'image/jpeg')
    return bytes.length >= 4 && bytes.subarray(0, 3).toString('hex') === 'ffd8ff' && bytes.subarray(-2).toString('hex') === 'ffd9'
  return bytes.length >= 20 && bytes.toString('ascii', 0, 4) === 'RIFF' && bytes.toString('ascii', 8, 12) === 'WEBP' && bytes.readUInt32LE(4) + 8 === bytes.length
}

/** 审计记录仅含操作字段与资源标识，不记录密码、token、头像内容或联系信息。 */
export async function recordAudit(context: ServContext, action: string, resource: string, resourceId?: string, details?: Record<string, unknown>): Promise<void> {
  const result = await audit.log({
    userId: context.session?.userId,
    action,
    resource,
    resourceId,
    details,
    ipAddress: context.ip,
    userAgent: context.userAgent,
  })
  if (!result.success)
    core.logger.error('Failed to record audit event', { action, resource, error: result.error.code })
}
