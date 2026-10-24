import { apiContract } from '@h-ai/api-contract'
import { z } from 'zod'
import {
  AdminAuditInputSchema,
  AdminAuditSchema,
  AdminCreateUserSchema,
  AdminRoleSchema,
  AdminUpdateUserSchema,
  AdminUserSchema,
  AdminUsersInputSchema,
  AdminUsersSchema,
  AvatarOutputSchema,
  AvatarUploadSchema,
  ProfileUpdateSchema,
} from './admin-schemas.js'

/** mini 管理示例的业务路径；初始 admin/user 角色可按实际需求调整。 */
export const adminContract = {
  users: {
    list: apiContract.route({ method: 'POST', path: '/admin/users/list', tags: ['admin'] })
      .input(AdminUsersInputSchema)
      .output(apiContract.haiResultSchema(AdminUsersSchema)),
    create: apiContract.route({ method: 'POST', path: '/admin/users/create', tags: ['admin'] })
      .input(AdminCreateUserSchema)
      .output(apiContract.haiResultSchema(AdminUserSchema)),
    update: apiContract.route({ method: 'POST', path: '/admin/users/update', tags: ['admin'] })
      .input(AdminUpdateUserSchema)
      .output(apiContract.haiResultSchema(AdminUserSchema)),
    delete: apiContract.route({ method: 'POST', path: '/admin/users/delete', tags: ['admin'] })
      .input(z.object({ id: z.string().min(1) }).strict())
      .output(apiContract.haiResultSchema(z.object({ deleted: z.literal(true) }))),
    avatar: apiContract.route({ method: 'POST', path: '/admin/users/avatar', tags: ['admin'] })
      .input(AvatarUploadSchema.extend({ id: z.string().min(1) }))
      .output(apiContract.haiResultSchema(AvatarOutputSchema)),
  },
  roles: {
    list: apiContract.route({ method: 'POST', path: '/admin/roles/list', tags: ['admin'] })
      .output(apiContract.haiResultSchema(z.object({ items: z.array(AdminRoleSchema) }))),
    update: apiContract.route({ method: 'POST', path: '/admin/roles/update', tags: ['admin'] })
      .input(z.object({ id: z.string().min(1), description: z.string().max(300) }).strict())
      .output(apiContract.haiResultSchema(AdminRoleSchema)),
  },
  audit: {
    list: apiContract.route({ method: 'POST', path: '/admin/audit/list', tags: ['admin'] })
      .input(AdminAuditInputSchema)
      .output(apiContract.haiResultSchema(AdminAuditSchema)),
  },
}

export const profileContract = {
  update: apiContract.route({ method: 'POST', path: '/profile/update', tags: ['profile'] })
    .input(ProfileUpdateSchema)
    .output(apiContract.haiResultSchema(AdminUserSchema)),
  avatar: apiContract.route({ method: 'POST', path: '/profile/avatar', tags: ['profile'] })
    .input(AvatarUploadSchema)
    .output(apiContract.haiResultSchema(AvatarOutputSchema)),
}

/** 复用框架认证契约；注册、通知和登录方式由 IAM 配置控制。 */
export const authContract = {
  publicConfig: apiContract.iam.auth.publicConfig,
  register: apiContract.iam.auth.register,
  loginWithLdap: apiContract.iam.auth.loginWithLdap,
  loginWithOtp: apiContract.iam.auth.loginWithOtp,
  sendOtp: apiContract.iam.auth.sendOtp,
  requestPasswordReset: apiContract.iam.auth.requestPasswordReset,
  confirmPasswordReset: apiContract.iam.auth.confirmPasswordReset,
  login: apiContract.iam.auth.login,
  logout: apiContract.iam.auth.logout,
  currentUser: apiContract.iam.auth.currentUser,
  refresh: apiContract.iam.auth.refresh,
  changePassword: apiContract.iam.auth.changePassword,
}
