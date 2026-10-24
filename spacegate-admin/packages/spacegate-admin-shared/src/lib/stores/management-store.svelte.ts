import type { IamPublicConfig } from '@h-ai/api-contract'
import type { HaiResult } from '@h-ai/core'
import type { ChangePasswordFormData, UserProfileSubmitData } from '@h-ai/ui'
import type { AdminAuditLog, AdminCreateUser, AdminRole, AdminUpdateUser, AdminUser, AvatarUpload, ProfileUpdate } from 'spacegate-admin-contract'
import type { AppInfo, ManagementApiOptions } from '../api/api-client.js'
import type { HashRouter } from '../navigation/hash-router.svelte.js'
import { HaiApiClientError } from '@h-ai/api-client'
import { setGlobalLocale } from '@h-ai/ui/i18n'
import { SvelteDate } from 'svelte/reactivity'
import { initializeManagementApi, managementApi, resolveAvatarUrl } from '../api/api-client.js'
import * as m from '../paraglide/messages.js'
import { getLocale, locales, setLocale } from '../paraglide/runtime.js'
import { AVAILABLE_THEMES, DEFAULT_THEME, themeStore } from './theme-store.svelte.js'

type ResultData<T> = T extends { success: true, data: infer D } ? D : never
export type CurrentUser = ResultData<Awaited<ReturnType<typeof managementApi.iam.auth.currentUser>>>
/** 页面目录对应的相对路径；例如 `reports/weekly`，新增页面无需修改类型或路由表。 */
export type ManagementPage = string

/** 页面状态和业务操作在共享 store 中收敛，宿主端仅装配平台能力。 */
export class ManagementStore {
  /**
   * 绑定当前工作区的页面路由；服务端继续验证各 API 的权限。
   * @example new ManagementStore(new HashRouter(Object.keys(pages), '/users'))
   */
  constructor(router: HashRouter) {
    this.router = router
  }

  private readonly router: HashRouter
  info = $state<AppInfo | null>(null)
  authConfig = $state<IamPublicConfig | null>(null)
  user = $state<CurrentUser | null>(null)
  page = $state<ManagementPage>('settings')
  locale = $state(getLocale())
  initializing = $state(true)
  loading = $state(false)
  saving = $state(false)
  error = $state('')
  notice = $state('')
  users = $state<AdminUser[]>([])
  usersTotal = $state(0)
  usersPage = $state(1)
  usersSearch = $state('')
  usersStatus = $state('all')
  roles = $state<AdminRole[]>([])
  audit = $state<AdminAuditLog[]>([])
  auditTotal = $state(0)
  auditPage = $state(1)
  auditAction = $state('')
  auditResource = $state('')
  auditStartDate = $state('')
  auditEndDate = $state('')
  readonly pageSize = 20
  private requestVersion = 0
  private sessionEpoch = 0
  private onLocaleChange?: (locale: string) => void

  get isAdmin(): boolean {
    return this.user?.roles.includes('admin') ?? false
  }

  async initialize(options: ManagementApiOptions, onLocaleChange?: (locale: string) => void): Promise<void> {
    this.onLocaleChange = onLocaleChange
    this.initializing = true
    this.error = ''
    try {
      const init = await initializeManagementApi({ ...options, onSessionExpired: () => this.expireSession() })
      if (!init.success) {
        this.error = init.error.code === HaiApiClientError.CONFIG_ERROR.code ? m.api_not_configured() : m.home_error()
        return
      }
      const info = await managementApi.app.info()
      if (!info.success) {
        this.error = m.home_error()
        return
      }
      this.info = info.data
      const authConfig = await managementApi.iam.auth.publicConfig()
      if (!authConfig.success) {
        this.error = authConfig.error.message
        return
      }
      this.authConfig = authConfig.data
      this.applyLocale(info.data.defaultLocale)
      // 默认 Cookie 存储通过 api-client 的加密刷新链路恢复会话。
      const me = await managementApi.iam.auth.currentUser()
      if (me.success) {
        this.setUser(me.data)
        await this.navigate(this.router.path.slice(1))
      }
      else if (me.error.httpStatus !== 401) {
        this.error = m.operation_failed()
      }
    }
    catch {
      this.error = m.home_error()
    }
    finally {
      this.initializing = false
    }
  }

  async close(): Promise<void> {
    this.requestVersion++
    this.sessionEpoch++
    this.loading = false
    await managementApi.close()
  }

  async authenticated(): Promise<void> {
    const me = await this.request(() => managementApi.iam.auth.currentUser())
    if (!me?.success)
      return
    this.setUser(me.data)
    await this.navigate(this.router.path.slice(1))
  }

  async logout(): Promise<void> {
    await this.mutate(async () => {
      const result = await this.request(() => managementApi.iam.auth.logout({}))
      if (!result?.success)
        return
      await managementApi.auth.clear()
      this.clearSession()
    })
  }

  async navigate(page: ManagementPage): Promise<void> {
    if (!this.user)
      return
    this.requestVersion++
    this.loading = false
    this.page = this.isAdmin ? page : 'settings'
    if (this.router.path !== `/${this.page}`)
      this.router.navigate(`/${this.page}`)
    this.page = this.router.path.slice(1)
    this.error = ''
    this.notice = ''
    if (this.page === 'users')
      await this.loadUsers()
    else if (this.page === 'roles')
      await this.loadRoles()
    else if (this.page === 'audit')
      await this.loadAudit()
  }

  async loadUsers(page = 1): Promise<void> {
    await this.load(async (version) => {
      const result = await this.request(() => managementApi.admin.users.list({
        page,
        pageSize: this.pageSize,
        search: this.usersSearch.trim() || undefined,
        enabled: this.usersStatus === 'all' ? undefined : this.usersStatus === 'enabled',
      }), version)
      if (result?.success && version === this.requestVersion) {
        this.users = result.data.items.map(user => ({ ...user, avatarUrl: resolveAvatarUrl(user.avatarUrl) }))
        this.usersTotal = result.data.total
        this.usersPage = result.data.page
      }
    })
  }

  async loadRoles(): Promise<void> {
    await this.load(async (version) => {
      const result = await this.request(() => managementApi.admin.roles.list(), version)
      if (result?.success && version === this.requestVersion)
        this.roles = result.data.items
    })
  }

  async loadAudit(page = 1): Promise<void> {
    if (this.auditStartDate && this.auditEndDate && this.auditStartDate > this.auditEndDate) {
      this.error = m.audit_date_invalid()
      return
    }
    await this.load(async (version) => {
      const result = await this.request(() => managementApi.admin.audit.list({
        page,
        pageSize: this.pageSize,
        action: this.auditAction.trim() || undefined,
        resource: this.auditResource.trim() || undefined,
        startDate: this.auditStartDate ? new SvelteDate(`${this.auditStartDate}T00:00:00`).toISOString() : undefined,
        endDate: this.auditEndDate ? new SvelteDate(`${this.auditEndDate}T23:59:59.999`).toISOString() : undefined,
      }), version)
      if (result?.success && version === this.requestVersion) {
        this.audit = result.data.items
        this.auditTotal = result.data.total
        this.auditPage = result.data.page
      }
    })
  }

  async saveUser(data: AdminCreateUser | AdminUpdateUser): Promise<boolean> {
    let saved = false
    await this.mutate(async () => {
      const result = 'id' in data
        ? await this.request(() => managementApi.admin.users.update(data))
        : await this.request(() => managementApi.admin.users.create(data))
      if (!result?.success)
        return
      saved = true
      await this.loadUsers(this.usersPage)
      this.notice = m.saved()
    })
    return saved
  }

  async deleteUser(user: AdminUser): Promise<boolean> {
    let deleted = false
    await this.mutate(async () => {
      const result = await this.request(() => managementApi.admin.users.delete({ id: user.id }))
      if (!result?.success)
        return
      deleted = true
      const page = this.users.length === 1 ? Math.max(1, this.usersPage - 1) : this.usersPage
      await this.loadUsers(page)
      this.notice = m.user_deleted()
    })
    return deleted
  }

  async saveRole(id: string, description: string): Promise<boolean> {
    let saved = false
    await this.mutate(async () => {
      const result = await this.request(() => managementApi.admin.roles.update({ id, description }))
      if (!result?.success)
        return
      saved = true
      await this.loadRoles()
      this.notice = m.saved()
    })
    return saved
  }

  async saveProfile(data: UserProfileSubmitData): Promise<void> {
    await this.savePreferences({ displayName: data.displayName?.trim() ?? '', email: data.email?.trim() ?? '' })
  }

  async savePreferences(data: ProfileUpdate): Promise<void> {
    await this.mutate(async () => {
      const result = await this.request(() => managementApi.profile.update(data))
      if (!result?.success)
        return
      const me = await this.request(() => managementApi.iam.auth.currentUser())
      if (me?.success)
        this.setUser(me.data)
      this.notice = m.saved()
    })
  }

  async changePassword(data: ChangePasswordFormData): Promise<void> {
    await this.mutate(async () => {
      if (!data.oldPassword || data.newPassword === data.oldPassword || data.newPassword !== data.confirmPassword || data.newPassword.length < 8) {
        this.error = m.validation_failed()
        return
      }
      const oldPassword = data.oldPassword
      const result = await this.request(() => managementApi.iam.auth.changePassword({ oldPassword, newPassword: data.newPassword }))
      if (!result?.success)
        return
      await managementApi.auth.clear()
      this.clearSession()
      this.notice = m.password_changed()
    })
  }

  async uploadAvatar(file: File, userId?: string): Promise<void> {
    await this.mutate(async () => {
      const payload = await this.readAvatar(file)
      if (!payload)
        return
      const result = userId
        ? await this.request(() => managementApi.admin.users.avatar({ id: userId, ...payload }))
        : await this.request(() => managementApi.profile.avatar(payload))
      if (!result?.success)
        return
      if (!userId || userId === this.user?.id) {
        const me = await this.request(() => managementApi.iam.auth.currentUser())
        if (me?.success)
          this.setUser(me.data)
      }
      if (userId)
        await this.loadUsers(this.usersPage)
      this.notice = m.avatar_saved()
    })
  }

  formatDate(value: string): string {
    return new Intl.DateTimeFormat(this.locale, { dateStyle: 'medium', timeStyle: 'short' }).format(new SvelteDate(value))
  }

  private async readAvatar(file: File): Promise<AvatarUpload | undefined> {
    if (file.size > 512 * 1024 || !['image/png', 'image/jpeg', 'image/webp'].includes(file.type)) {
      this.error = m.avatar_invalid()
      return undefined
    }
    const bytes = new Uint8Array(await file.arrayBuffer())
    let binary = ''
    for (const byte of bytes)
      binary += String.fromCharCode(byte)
    return { contentType: file.type as AvatarUpload['contentType'], base64: btoa(binary) }
  }

  private setUser(user: CurrentUser): void {
    if (this.user?.id !== user.id)
      this.sessionEpoch++
    this.user = { ...user, avatarUrl: resolveAvatarUrl(user.avatarUrl) }
    this.applyLocale(typeof user.metadata?.locale === 'string' ? user.metadata.locale : this.info?.defaultLocale ?? getLocale())
    if (typeof user.metadata?.theme === 'string' && AVAILABLE_THEMES.includes(user.metadata.theme))
      themeStore.set(user.metadata.theme)
    else
      themeStore.set(DEFAULT_THEME)
  }

  private applyLocale(value: string): void {
    const locale = locales.find(locale => locale === value)
    if (!locale)
      return
    this.locale = locale
    setLocale(locale, { reload: false })
    setGlobalLocale(locale)
    if (typeof document !== 'undefined')
      document.documentElement.lang = locale
    this.onLocaleChange?.(locale)
  }

  private clearSession(): void {
    this.requestVersion++
    this.sessionEpoch++
    this.loading = false
    this.user = null
    this.users = []
    this.roles = []
    this.audit = []
    this.page = 'settings'
  }

  private expireSession(): void {
    if (!this.user)
      return
    this.clearSession()
    this.error = m.session_expired()
    void managementApi.auth.clear()
  }

  private async request<T>(task: () => Promise<HaiResult<T>>, version?: number): Promise<HaiResult<T> | undefined> {
    const epoch = this.sessionEpoch
    const isCurrent = () => epoch === this.sessionEpoch && (version === undefined || version === this.requestVersion)
    try {
      const result = await task()
      if (!isCurrent())
        return undefined
      if (!result.success) {
        if (result.error.httpStatus === 401 && this.user) {
          this.expireSession()
        }
        else if (result.error.httpStatus === 403 && this.user) {
          const current = await managementApi.iam.auth.currentUser()
          if (!isCurrent())
            return undefined
          if (current.success) {
            this.setUser(current.data)
            if (!this.isAdmin) {
              this.requestVersion++
              this.loading = false
              this.users = []
              this.roles = []
              this.audit = []
              this.page = 'settings'
            }
          }
          this.error = m.forbidden()
        }
        else {
          this.error = result.error.httpStatus === 403 ? m.forbidden() : result.error.httpStatus === 409 ? m.conflict() : result.error.httpStatus === 400 || result.error.httpStatus === 422 ? m.validation_failed() : m.operation_failed()
        }
      }
      return result
    }
    catch {
      if (isCurrent())
        this.error = m.network_error()
      return undefined
    }
  }

  private async load(task: (version: number) => Promise<void>): Promise<void> {
    const version = ++this.requestVersion
    this.loading = true
    this.error = ''
    await task(version)
    if (version === this.requestVersion)
      this.loading = false
  }

  private async mutate(task: () => Promise<void>): Promise<void> {
    if (this.saving)
      return
    this.saving = true
    this.error = ''
    this.notice = ''
    try {
      await task()
    }
    catch {
      this.error = m.network_error()
    }
    finally {
      this.saving = false
    }
  }
}
