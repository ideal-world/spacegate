import type { ServContext } from '@h-ai/serv'
import { audit } from '@h-ai/audit'
import { err, HaiCommonError, ok } from '@h-ai/core'
import { iam } from '@h-ai/iam'
import { serv } from '@h-ai/serv'
import { createIamProcedures } from '@h-ai/serv/features/iam'
import { adminContract, authContract, profileContract } from 'spacegate-admin-contract'
import {
  createAdminUser,
  getAdminRoles,
  isRoleCode,
  recordAudit,
  toAdminUser,
  updateAdminUser,
  updateProfile,
  uploadAvatar,
  withAdminMutation,
} from '../admin-service.js'
import { serverM } from '../messages.js'

/** 身份认证沿用框架公共契约与 refresh-cookie 传输。 */
export function createAuthProcedures() {
  const { currentUser, refresh, publicConfig, loginWithOtp, loginWithLdap, register, sendOtp, requestPasswordReset, confirmPasswordReset } = createIamProcedures({ iam }).auth
  const { login, logout, changePassword } = authContract
  // 仅覆写需要业务审计的认证操作，其余直接复用框架；未选认证入口不挂载。
  const audited = serv.implement({ login, logout, changePassword }).context<ServContext>().route('login', async ({ input, context }) => {
    const result = await iam.auth.login(input)
    if (!result.success)
      return result
    await recordAudit({ ...context, session: { userId: result.data.user.id, roles: result.data.roles, permissions: result.data.permissions } }, 'login', 'auth')
    return result
  }).route('logout', async ({ input, context }) => {
    const token = input.accessToken ?? context.accessToken
    if (!token)
      return err(HaiCommonError.UNAUTHORIZED, serverM('unauthorized', { locale: context.locale }))
    const result = await iam.auth.logout(token)
    if (!result.success)
      return result
    if (context.session)
      await recordAudit(context, 'logout', 'auth')
    return result
  }).route('changePassword').auth().handle(async ({ input, context }) => {
    const result = await iam.user.changeCurrentUserPassword(context.accessToken ?? '', input.oldPassword, input.newPassword)
    if (!result.success)
      return result
    await recordAudit(context, 'password-change', 'profile', context.session.userId)
    return result
  }).build()
  return { ...audited, currentUser, refresh, publicConfig, loginWithOtp, loginWithLdap, register, sendOtp, requestPasswordReset, confirmPasswordReset }
}

export function createAdminProcedures() {
  return serv.implement(adminContract).context<ServContext>().route('users.list').role('admin').handle(async ({ input }) => {
    const users = await iam.user.listUsers(input)
    if (!users.success)
      return users
    const roles = await iam.authz.getUserRolesForMany(users.data.items.map(user => user.id))
    if (!roles.success)
      return roles
    return ok({ ...users.data, items: users.data.items.map(user => toAdminUser(user, roles.data.get(user.id) ?? [])) })
  }).route('users.create').role('admin').handle(({ input, context }) => withAdminMutation(context, () => createAdminUser(input, context))).route('users.update').role('admin').handle(({ input, context }) => withAdminMutation(context, () => updateAdminUser(input, context))).route('users.delete').role('admin').handle(({ input, context }) => withAdminMutation(context, async () => {
    if (context.session.userId === input.id)
      return err(HaiCommonError.FORBIDDEN, serverM('selfProtection', { locale: context.locale }))
    const result = await iam.user.deleteUser(input.id)
    if (!result.success)
      return result
    await recordAudit(context, 'delete', 'users', input.id)
    return ok({ deleted: true as const })
  })).route('users.avatar').role('admin').handle(({ input, context }) => withAdminMutation(context, () => uploadAvatar(input.id, input, context))).route('roles.list').role('admin').handle(() => getAdminRoles()).route('roles.update').role('admin').handle(({ input, context }) => withAdminMutation(context, async () => {
    const role = await iam.authz.getRole(input.id)
    if (!role.success)
      return role
    if (!role.data || !isRoleCode(role.data.code))
      return err(HaiCommonError.NOT_FOUND, serverM('notFound', { locale: context.locale }))
    const updated = await iam.authz.updateRole(input.id, { description: input.description })
    if (!updated.success)
      return updated
    const roles = await getAdminRoles()
    if (!roles.success)
      return roles
    const result = roles.data.items.find(item => item.id === input.id)
    if (!result)
      return err(HaiCommonError.NOT_FOUND, serverM('notFound', { locale: context.locale }))
    await recordAudit(context, 'update', 'roles', input.id, { fields: ['description'] })
    return ok(result)
  })).route('audit.list').role('admin').handle(async ({ input, context }) => {
    if (input.startDate && input.endDate && input.startDate > input.endDate)
      return err(HaiCommonError.VALIDATION_ERROR, serverM('invalidDateRange', { locale: context.locale }))
    const result = await audit.list({ ...input, startDate: input.startDate ? new Date(input.startDate) : undefined, endDate: input.endDate ? new Date(input.endDate) : undefined })
    if (!result.success)
      return result
    return ok({ ...result.data, items: result.data.items.map(item => ({ ...item, createdAt: new Date(item.createdAt).toISOString() })) })
  }).build()
}

export function createProfileProcedures() {
  return serv.implement(profileContract).context<ServContext>().route('update').auth().handle(({ input, context }) => updateProfile(input, context)).route('avatar').auth().handle(({ input, context }) => uploadAvatar(context.session.userId, input, context)).build()
}
