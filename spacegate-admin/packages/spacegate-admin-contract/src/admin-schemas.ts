import { z } from 'zod'

export const AdminRoleCodeSchema = z.enum(['admin', 'user'])
export const AdminPageInputSchema = z.object({
  page: z.number().int().min(1).default(1),
  pageSize: z.number().int().min(1).max(100).default(20),
})
export const AdminUserSchema = z.object({
  id: z.string(),
  username: z.string(),
  email: z.string().optional(),
  displayName: z.string().optional(),
  avatarUrl: z.string().optional(),
  enabled: z.boolean(),
  roles: z.array(AdminRoleCodeSchema),
  createdAt: z.string().datetime(),
  updatedAt: z.string().datetime(),
})
export const AdminUsersInputSchema = AdminPageInputSchema.extend({
  search: z.string().max(100).optional(),
  enabled: z.boolean().optional(),
})
export const AdminUsersSchema = z.object({
  items: z.array(AdminUserSchema),
  total: z.number(),
  page: z.number(),
  pageSize: z.number(),
})
export const AdminCreateUserSchema = z.object({
  username: z.string().min(3).max(64).regex(/^[\w-]+$/),
  password: z.string().min(1),
  email: z.email().optional(),
  displayName: z.string().max(100).optional(),
  role: AdminRoleCodeSchema.default('user'),
  enabled: z.boolean().default(true),
}).strict()
export const AdminUpdateUserSchema = z.object({
  id: z.string().min(1),
  email: z.union([z.email(), z.literal('')]).optional(),
  displayName: z.string().max(100).optional(),
  enabled: z.boolean().optional(),
  role: AdminRoleCodeSchema.optional(),
}).strict()
export const AdminRoleSchema = z.object({
  id: z.string(),
  code: AdminRoleCodeSchema,
  name: z.string(),
  description: z.string().optional(),
  userCount: z.number(),
})
export const AdminAuditInputSchema = AdminPageInputSchema.extend({
  userId: z.string().max(100).optional(),
  action: z.string().max(50).optional(),
  resource: z.string().max(50).optional(),
  startDate: z.string().datetime().optional(),
  endDate: z.string().datetime().optional(),
})
export const AdminAuditLogSchema = z.object({
  id: z.string(),
  userId: z.string().nullable(),
  username: z.string().nullable(),
  action: z.string(),
  resource: z.string(),
  resourceId: z.string().nullable(),
  details: z.string().nullable(),
  ipAddress: z.string().nullable(),
  userAgent: z.string().nullable(),
  createdAt: z.string().datetime(),
})
export const AdminAuditSchema = z.object({
  items: z.array(AdminAuditLogSchema),
  total: z.number(),
  page: z.number(),
  pageSize: z.number(),
})
export const ProfileUpdateSchema = z.object({
  displayName: z.string().max(100).optional(),
  email: z.union([z.email(), z.literal('')]).optional(),
  locale: z.enum(['zh-CN', 'en-US', 'ja-JP']).optional(),
  theme: z.string().min(1).max(40).regex(/^[\w-]+$/).optional(),
}).strict()
/** 512 KiB 头像以 JSON 传输，上传及响应均经过统一加密。 */
export const AvatarUploadSchema = z.object({
  contentType: z.enum(['image/png', 'image/jpeg', 'image/webp']),
  base64: z.string().min(1).max(699052).regex(/^[A-Z\d+/]+={0,2}$/i),
}).strict()
export const AvatarOutputSchema = z.object({ avatarUrl: z.string().describe('Generated relative asset path') })

export type AdminUser = z.infer<typeof AdminUserSchema>
export type AdminRole = z.infer<typeof AdminRoleSchema>
export type AdminRoleCode = z.infer<typeof AdminRoleCodeSchema>
export type AdminAuditLog = z.infer<typeof AdminAuditLogSchema>
export type AdminCreateUser = z.infer<typeof AdminCreateUserSchema>
export type AdminUpdateUser = z.infer<typeof AdminUpdateUserSchema>
export type ProfileUpdate = z.infer<typeof ProfileUpdateSchema>
export type AvatarUpload = z.infer<typeof AvatarUploadSchema>
