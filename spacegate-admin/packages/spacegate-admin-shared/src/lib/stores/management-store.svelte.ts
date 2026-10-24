import type { IamPublicConfig } from '@h-ai/api-contract'
import type { HaiResult } from '@h-ai/core'
import type { ChangePasswordFormData, UserProfileSubmitData } from '@h-ai/ui'
import type {
  AdminAuditLog,
  AdminCreateUser,
  AdminRole,
  AdminUpdateUser,
  AdminUser,
  AvatarUpload,
  PluginCode,
  PluginConfig,
  PluginInstanceId,
  ProfileUpdate,
  SpacegateAiQueueFilters,
  SpacegateAiQueueResolveInput,
  SpacegatePluginPreviewWasmSchemaInput,
} from 'spacegate-admin-contract'
import type { AppInfo, ManagementApiOptions } from '../api/api-client.js'
import type { HashRouter } from '../navigation/hash-router.svelte.js'
import type { JsonRecord, PluginInstance } from '../spacegate/model.js'
import { HaiApiClientError } from '@h-ai/api-client'
import { setGlobalLocale } from '@h-ai/ui/i18n'
import { SvelteDate } from 'svelte/reactivity'
import { initializeManagementApi, managementApi, resolveAvatarUrl } from '../api/api-client.js'
import * as m from '../paraglide/messages.js'
import { getLocale, locales, setLocale } from '../paraglide/runtime.js'
import {
  asRecord,
  asRecordArray,
  cloneRecord,
  gatewayNameOf,
  parseServerVersion,
  pluginInstanceOf,
  pluginKey,
  routeNameOf,
} from '../spacegate/model.js'
import { AVAILABLE_THEMES, DEFAULT_THEME, themeStore } from './theme-store.svelte.js'

type ResultData<T> = T extends { success: true, data: infer D } ? D : never
/** 路由列表项；name 与 Rust 返回 key 一致，route 保留完整配置。 */
export interface SpacegateRouteItem { name: string, route: unknown }
/** 网关列表摘要；详情读取失败时计数为 0，并保留名称供用户重试。 */
export interface SpacegateGatewaySummary { name: string, gateway: JsonRecord | null, routeCount: number, pluginCount: number }
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
  // SpaceGate 配置快照；version 始终来自最近一次用于编辑的读取或写成功结果。
  gateways = $state<string[]>([])
  gatewaySummaries = $state<SpacegateGatewaySummary[]>([])
  gateway = $state<JsonRecord | null>(null)
  gatewayConfig = $state<JsonRecord | null>(null)
  version = $state(0)
  /** 编辑器读取时的版本快照；后台读取推进全局 version 时不得改变它。 */
  gatewayEditVersion = $state(0)
  routeEditVersion = $state(0)
  pluginEditVersion = $state(0)
  routes = $state<SpacegateRouteItem[]>([])
  route = $state<JsonRecord | null>(null)
  plugins = $state<PluginInstance[]>([])
  pluginCatalog = $state<string[]>([])
  pluginAttrs = $state<JsonRecord[]>([])
  plugin = $state<PluginInstance | null>(null)
  pluginSchema = $state<unknown>(null)
  wasmSchema = $state<unknown>(null)
  wasmSchemaPreview = $state<unknown>(null)
  instances = $state<string[]>([])
  health = $state<Record<string, boolean>>({})
  aiRules = $state<JsonRecord[]>([])
  aiSchema = $state<unknown>(null)
  aiReadme = $state('')
  aiResolution = $state<unknown>(null)
  conflict = $state(false)
  conflictServerVersion = $state<number | null>(null)
  selectedGateway = $state('')
  selectedRoute = $state('')
  selectedPlugin = $state('')
  selectedPluginCode = $state('')
  /** 显式冲突刷新计数；页面可据此关闭已过期的编辑器。 */
  reloadEpoch = $state(0)
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
    this.conflict = false
    this.conflictServerVersion = null
    if (!this.isAdmin)
      return
    if (this.page === 'overview') {
      await this.loadOverview()
}
    else if (this.page === 'gateways') {
      await this.loadGateways()
}
    else if (this.page === 'routes') {
      await this.loadGateways()
      await this.loadRoutes()
    }
    else if (this.page === 'plugins') {
      await this.loadPlugins()
}
    else if (this.page === 'ai-queue') {
      await this.loadAiQueue()
}
    else if (this.page === 'instances') {
      await this.loadInstances()
}
    else if (this.page === 'users') {
      await this.loadUsers()
}
    else if (this.page === 'roles') {
      await this.loadRoles()
}
    else if (this.page === 'audit') {
      await this.loadAudit()
}
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

  /** 切换当前网关；同时刷新网关详情和路由列表。 */
  async selectGateway(name: string): Promise<void> {
    if (!this.isAdmin)
      return
    this.selectedGateway = name
    this.selectedRoute = ''
    await this.loadGateway(name)
    await this.loadRoutes(name)
  }

  /** 概览：网关/插件/实例计数 + 当前网关和路由。 */
  async loadOverview(): Promise<void> {
    if (!this.isAdmin)
      return
    await this.load(async (version) => {
      const [names, pluginList, instanceList, healthResult] = await Promise.all([
        this.request(() => managementApi.spacegate.config.names({}), version, true),
        this.request(() => managementApi.spacegate.plugin.list({}), version, true),
        this.request(() => managementApi.spacegate.discovery.instances({}), version, true),
        this.request(() => managementApi.spacegate.discovery.health({}), version, true),
      ])
      if (version !== this.requestVersion)
        return
      if (names?.success) {
        this.gateways = names.data.items
        this.applyVersion(names.data.version)
      }
      if (pluginList?.success) {
        this.plugins = pluginList.data.items.map(item => pluginInstanceOf(item)).filter((item): item is PluginInstance => item !== null)
        this.applyVersion(pluginList.data.version)
      }
      if (instanceList?.success) {
        this.instances = instanceList.data.items
        this.applyVersion(instanceList.data.version)
      }
      if (healthResult?.success) {
        this.health = healthResult.data.health
        this.applyVersion(healthResult.data.version)
      }
      if (!names?.success)
        return
      if (!this.selectedGateway || !names.data.items.includes(this.selectedGateway))
        this.selectedGateway = names.data.items[0] ?? ''
      if (!this.selectedGateway) {
        this.gateway = null
        this.gatewayConfig = null
        this.routes = []
        return
      }
      const [gatewayResult, routeResult] = await Promise.all([
        this.request(() => managementApi.spacegate.gateway.get({ name: this.selectedGateway }), version, true),
        this.request(() => managementApi.spacegate.route.list({ gateway: this.selectedGateway }), version, true),
      ])
      if (version !== this.requestVersion)
        return
      if (gatewayResult?.success) {
        this.gateway = asRecord(gatewayResult.data.gateway)
        this.gatewayConfig = cloneRecord(gatewayResult.data.gateway)
        this.applyVersion(gatewayResult.data.version)
      }
      if (routeResult?.success) {
        this.routes = routeResult.data.items.map(item => ({ name: item.name, route: item.route }))
        this.applyVersion(routeResult.data.version)
      }
    })
  }

  /** 网关列表：名称 + 每个网关的详情与路由数。 */
  async loadGateways(): Promise<void> {
    if (!this.isAdmin)
      return
    await this.load(async (version) => {
      const names = await this.request(() => managementApi.spacegate.config.names({}), version, true)
      if (!names?.success || version !== this.requestVersion)
        return
      this.gateways = names.data.items
      this.applyVersion(names.data.version)
      if (!this.selectedGateway || !names.data.items.includes(this.selectedGateway))
        this.selectedGateway = names.data.items[0] ?? ''
      const summaries = await Promise.all(names.data.items.map(async (name) => {
        const [gatewayResult, routeResult] = await Promise.all([
          this.request(() => managementApi.spacegate.gateway.get({ name }), version, true),
          this.request(() => managementApi.spacegate.route.list({ gateway: name }), version, true),
        ])
        if (gatewayResult?.success)
          this.applyVersion(gatewayResult.data.version)
        if (routeResult?.success)
          this.applyVersion(routeResult.data.version)
        const gateway = gatewayResult?.success ? asRecord(gatewayResult.data.gateway) : null
        return {
          name,
          gateway,
          routeCount: routeResult?.success ? routeResult.data.items.length : 0,
          pluginCount: asRecordArray(gateway?.plugins).length,
        }
      }))
      if (version === this.requestVersion)
        this.gatewaySummaries = summaries
    })
  }

  /** 读取当前网关完整配置；编辑器据此建立版本快照。 */
  async loadGateway(name = this.selectedGateway): Promise<void> {
    if (!this.isAdmin || !name)
      return
    await this.load(async (version) => {
      const result = await this.request(() => managementApi.spacegate.gateway.get({ name }), version, true)
      if (!result?.success || version !== this.requestVersion)
        return
      this.selectedGateway = name
      this.gateway = asRecord(result.data.gateway)
      this.gatewayConfig = cloneRecord(result.data.gateway)
      this.gatewayEditVersion = result.data.version
      this.applyVersion(result.data.version)
    })
  }

  /** 保存网关；create 决定 POST/PUT，成功后重新读取并以服务端新版本继续编辑。 */
  async saveGateway(create: boolean, name = this.selectedGateway, config?: JsonRecord): Promise<boolean> {
    if (!this.isAdmin)
      return false
    const gatewayName = (name || gatewayNameOf(config ?? this.gatewayConfig)).trim()
    if (!gatewayName) {
      this.error = m.spacegate_name_required()
      return false
    }
    const payload = cloneRecord(config ?? this.gatewayConfig)
    payload.name = gatewayName
    let saved = false
    await this.mutate(async () => {
      const result = await this.request(() => managementApi.spacegate.gateway.upsert({
        name: gatewayName,
        gateway: payload,
        version: create ? this.version : this.gatewayEditVersion,
        create,
      }), undefined, true)
      if (!result?.success)
        return
      saved = true
      this.gatewayEditVersion = result.data.version
      this.applyVersion(result.data.version)
      this.selectedGateway = gatewayName
      this.gatewayConfig = payload
      await this.loadGateway(gatewayName)
      await this.loadRoutes(gatewayName)
      const summary = this.gatewaySummaries.find(item => item.name === gatewayName)
      if (summary) {
        summary.gateway = payload
        summary.routeCount = this.routes.length
        summary.pluginCount = asRecordArray(payload.plugins).length
      }
      this.notice = m.saved()
    })
    return saved
  }

  /** 删除网关；具体级联行为由上游后端决定，页面确认文案不承诺删除路由。 */
  async deleteGateway(name = this.selectedGateway): Promise<boolean> {
    if (!this.isAdmin || !name)
      return false
    let deleted = false
    await this.mutate(async () => {
      const result = await this.request(() => managementApi.spacegate.gateway.delete({ name, version: this.gatewayEditVersion || this.version }), undefined, true)
      if (!result?.success)
        return
      deleted = true
      this.gatewayEditVersion = result.data.version
      this.applyVersion(result.data.version)
      if (this.selectedGateway === name) {
        this.selectedGateway = ''
        this.gateway = null
        this.gatewayConfig = null
        this.routes = []
      }
      await this.loadGateways()
      if (this.selectedGateway) {
        await this.loadGateway(this.selectedGateway)
        await this.loadRoutes(this.selectedGateway)
      }
      this.notice = m.saved()
    })
    return deleted
  }

  /** 读取网关路由列表；同时把当前网关切换为读取目标。 */
  async loadRoutes(gateway = this.selectedGateway): Promise<void> {
    if (!this.isAdmin || !gateway)
      return
    await this.load(async (version) => {
      const result = await this.request(() => managementApi.spacegate.route.list({ gateway }), version, true)
      if (!result?.success || version !== this.requestVersion)
        return
      this.selectedGateway = gateway
      this.routes = result.data.items.map(item => ({ name: item.name, route: item.route }))
      if (!this.routes.some(item => item.name === this.selectedRoute))
        this.selectedRoute = ''
      this.applyVersion(result.data.version)
    })
  }

  /** 读取单个路由完整配置；保留未知字段供编辑器往返。 */
  async loadRoute(name = this.selectedRoute, gateway = this.selectedGateway): Promise<void> {
    if (!this.isAdmin || !gateway || !name)
      return
    await this.load(async (version) => {
      const result = await this.request(() => managementApi.spacegate.route.get({ gateway, route: name }), version, true)
      if (!result?.success || version !== this.requestVersion)
        return
      this.selectedRoute = name
      this.route = asRecord(result.data.route)
      this.routeEditVersion = result.data.version
      this.applyVersion(result.data.version)
    })
  }

  /** 创建或更新路由；更新时路由名必须与读取时一致，改名走 renameRoute。 */
  async saveRoute(create: boolean, name = this.selectedRoute, config?: JsonRecord): Promise<boolean> {
    if (!this.isAdmin)
      return false
    if (!this.selectedGateway) {
      this.error = m.spacegate_gateway_required()
      return false
    }
    const routeName = (name || routeNameOf(config ?? this.route)).trim()
    if (!routeName) {
      this.error = m.spacegate_name_required()
      return false
    }
    const payload = cloneRecord(config ?? this.route)
    payload.route_name = routeName
    let saved = false
    await this.mutate(async () => {
      const result = await this.request(() => managementApi.spacegate.route.upsert({
        gateway: this.selectedGateway,
        route: routeName,
        config: payload,
        version: create ? this.version : this.routeEditVersion,
        create,
      }), undefined, true)
      if (!result?.success)
        return
      saved = true
      this.routeEditVersion = result.data.version
      this.applyVersion(result.data.version)
      this.selectedRoute = routeName
      this.route = payload
      await this.loadRoutes(this.selectedGateway)
      this.notice = m.saved()
    })
    return saved
  }

  /** 路由改名；提交完整配置，避免丢失规则、后端和插件绑定。 */
  async renameRoute(oldName: string, newName: string, config?: JsonRecord): Promise<boolean> {
    if (!this.isAdmin || !this.selectedGateway)
      return false
    const target = newName.trim()
    if (!target) {
      this.error = m.spacegate_name_required()
      return false
    }
    const payload = cloneRecord(config ?? this.route)
    payload.route_name = target
    let saved = false
    await this.mutate(async () => {
      const result = await this.request(() => managementApi.spacegate.route.rename({
        gateway: this.selectedGateway,
        route: oldName,
        newName: target,
        config: payload,
        version: this.routeEditVersion,
      }), undefined, true)
      if (!result?.success)
        return
      saved = true
      this.routeEditVersion = result.data.version
      this.applyVersion(result.data.version)
      this.selectedRoute = target
      this.route = payload
      await this.loadRoutes(this.selectedGateway)
      this.notice = m.saved()
    })
    return saved
  }

  /** 删除路由并刷新列表。 */
  async deleteRoute(name = this.selectedRoute): Promise<boolean> {
    if (!this.isAdmin || !this.selectedGateway || !name)
      return false
    let deleted = false
    await this.mutate(async () => {
      const result = await this.request(() => managementApi.spacegate.route.delete({
        gateway: this.selectedGateway,
        route: name,
        version: this.routeEditVersion || this.version,
      }), undefined, true)
      if (!result?.success)
        return
      deleted = true
      this.routeEditVersion = result.data.version
      this.applyVersion(result.data.version)
      if (this.selectedRoute === name) {
        this.selectedRoute = ''
        this.route = null
      }
      await this.loadRoutes(this.selectedGateway)
      this.notice = m.saved()
    })
    return deleted
  }

  /** 加载插件目录、属性与实例；code 为空时清空过滤并返回全部实例。 */
  async loadPlugins(code?: PluginCode): Promise<void> {
    if (!this.isAdmin)
      return
    this.selectedPluginCode = code ?? ''
    await this.load(async (version) => {
      const [catalog, attrs, list] = await Promise.all([
        this.request(() => managementApi.spacegate.plugin.catalog({}), version, true),
        this.request(() => managementApi.spacegate.plugin.attrAll({}), version, true),
        this.request(() => managementApi.spacegate.plugin.list(this.selectedPluginCode ? { code: this.selectedPluginCode } : {}), version, true),
      ])
      if (version !== this.requestVersion)
        return
      if (catalog?.success) {
        this.pluginCatalog = catalog.data.items
        this.applyVersion(catalog.data.version)
      }
      if (attrs?.success) {
        this.pluginAttrs = attrs.data.items.map(item => asRecord(item)).filter((item): item is JsonRecord => item !== null)
        this.applyVersion(attrs.data.version)
      }
      if (list?.success) {
        this.plugins = list.data.items.map(item => pluginInstanceOf(item)).filter((item): item is PluginInstance => item !== null)
        this.applyVersion(list.data.version)
        if (this.selectedPlugin && !this.plugins.some(item => pluginKey(item.ref) === this.selectedPlugin))
          this.plugin = null
      }
    })
  }

  /** 读取插件实例详情。 */
  async loadPlugin(id: PluginInstanceId): Promise<void> {
    if (!this.isAdmin)
      return
    await this.load(async (version) => {
      const result = await this.request(() => managementApi.spacegate.plugin.get({ id }), version, true)
      if (!result?.success || version !== this.requestVersion)
        return
      const instance = pluginInstanceOf(result.data.plugin)
      this.plugin = instance
      this.selectedPlugin = instance ? pluginKey(instance.ref) : ''
      if (instance)
        this.selectedPluginCode = instance.ref.code
      this.pluginEditVersion = result.data.version
      this.applyVersion(result.data.version)
    })
  }

  /** 保存插件实例；成功后按 code 重新拉取实例，版本来自写结果。 */
  async savePlugin(create: boolean, pluginConfig: PluginConfig): Promise<boolean> {
    if (!this.isAdmin)
      return false
    let saved = false
    await this.mutate(async () => {
      const result = await this.request(() => managementApi.spacegate.plugin.upsert({
        plugin: pluginConfig,
        version: create ? this.version : this.pluginEditVersion,
        create,
      }), undefined, true)
      if (!result?.success)
        return
      saved = true
      this.pluginEditVersion = result.data.version
      this.applyVersion(result.data.version)
      this.selectedPluginCode = pluginConfig.id.code
      this.selectedPlugin = pluginKey(pluginConfig.id)
      await this.loadPlugins(pluginConfig.id.code)
      this.plugin = this.plugins.find(item => pluginKey(item.ref) === this.selectedPlugin) ?? null
      this.notice = m.saved()
    })
    return saved
  }

  /** 删除插件实例；页面负责确认提示。 */
  async deletePlugin(id: PluginInstanceId): Promise<boolean> {
    if (!this.isAdmin)
      return false
    let deleted = false
    await this.mutate(async () => {
      const result = await this.request(() => managementApi.spacegate.plugin.delete({ id, version: this.pluginEditVersion || this.version }), undefined, true)
      if (!result?.success)
        return
      deleted = true
      this.pluginEditVersion = result.data.version
      this.applyVersion(result.data.version)
      this.plugin = null
      this.selectedPlugin = ''
      await this.loadPlugins(id.code)
      this.notice = m.saved()
    })
    return deleted
  }

  /** 刷新服务端插件属性缓存，同时更新本地属性列表。 */
  async refreshPlugins(): Promise<boolean> {
    if (!this.isAdmin)
      return false
    let refreshed = false
    await this.load(async (version) => {
      const result = await this.request(() => managementApi.spacegate.plugin.refresh({}), version, true)
      if (!result?.success || version !== this.requestVersion)
        return
      refreshed = true
      this.pluginAttrs = result.data.items.map(item => asRecord(item)).filter((item): item is JsonRecord => item !== null)
      this.applyVersion(result.data.version)
    })
    if (refreshed)
      this.notice = m.spacegate_plugin_refreshed()
    return refreshed
  }

  /** 读取运行时插件 schema，用于原生插件实例表单。 */
  async loadPluginSchema(code: PluginCode): Promise<void> {
    if (!this.isAdmin)
      return
    await this.load(async (version) => {
      const result = await this.request(() => managementApi.spacegate.plugin.schema({ code }), version, true)
      if (!result?.success || version !== this.requestVersion)
        return
      this.pluginSchema = result.data.schema
      this.applyVersion(result.data.version)
    })
  }

  /** 读取已保存 Wasm 实例的 schema。 */
  async loadWasmSchema(id: PluginInstanceId): Promise<void> {
    if (!this.isAdmin)
      return
    await this.load(async (version) => {
      const result = await this.request(() => managementApi.spacegate.plugin.wasmSchema({ id }), version, true)
      if (!result?.success || version !== this.requestVersion)
        return
      this.wasmSchema = result.data.schema
      this.applyVersion(result.data.version)
    })
  }

  /** 只读预览 Wasm 镜像内 schema；不参与版本控制，也不会自动保存。 */
  async previewWasmSchema(input: SpacegatePluginPreviewWasmSchemaInput): Promise<void> {
    if (!this.isAdmin)
      return
    await this.load(async (version) => {
      this.wasmSchemaPreview = null
      const result = await this.request(() => managementApi.spacegate.plugin.previewWasmSchema(input), version, true)
      if (!result?.success || version !== this.requestVersion)
        return
      this.wasmSchemaPreview = result.data.schema
    })
  }

  /** AI 队列规则、插件 schema 与说明；AI Gateway 数据不带配置版本。 */
  async loadAiQueue(filters?: SpacegateAiQueueFilters): Promise<void> {
    if (!this.isAdmin)
      return
    const query = filters ? Object.fromEntries(Object.entries(filters).filter(([, value]) => typeof value === 'string' && value.length > 0)) : {}
    await this.load(async (version) => {
      const [rules, schema, readme] = await Promise.all([
        this.request(() => managementApi.spacegate.aiQueue.rules({ filters: query }), version, true),
        this.request(() => managementApi.spacegate.aiQueue.schema({}), version, true),
        this.request(() => managementApi.spacegate.aiQueue.readme({}), version, true),
      ])
      if (version !== this.requestVersion)
        return
      if (rules?.success)
        this.aiRules = rules.data.items.map(item => asRecord(item)).filter((item): item is JsonRecord => item !== null)
      if (schema?.success)
        this.aiSchema = schema.data.schema
      if (readme?.success)
        this.aiReadme = readme.data.markdown
    })
  }

  /** 新增或更新 AI 队列规则，成功后刷新列表与 schema。 */
  async saveAiRule(rule: JsonRecord): Promise<boolean> {
    if (!this.isAdmin)
      return false
    let saved = false
    await this.mutate(async () => {
      const result = await this.request(() => managementApi.spacegate.aiQueue.upsert({ rule }), undefined, true)
      if (!result?.success)
        return
      saved = true
      await this.loadAiQueue()
      this.notice = m.saved()
    })
    return saved
  }

  /** 删除 AI 队列规则。 */
  async deleteAiRule(rule: JsonRecord): Promise<boolean> {
    if (!this.isAdmin)
      return false
    let deleted = false
    await this.mutate(async () => {
      const result = await this.request(() => managementApi.spacegate.aiQueue.delete({ rule }), undefined, true)
      if (!result?.success)
        return
      deleted = true
      await this.loadAiQueue()
      this.notice = m.saved()
    })
    return deleted
  }

  /** 解析当前请求命中的队列规则。 */
  async resolveAiRule(input: SpacegateAiQueueResolveInput): Promise<void> {
    if (!this.isAdmin)
      return
    await this.load(async (version) => {
      this.aiResolution = null
      const result = await this.request(() => managementApi.spacegate.aiQueue.resolve(input), version, true)
      if (!result?.success || version !== this.requestVersion)
        return
      this.aiResolution = result.data.resolution
    })
  }

  /** 实例 ID 与健康状态。 */
  async loadInstances(): Promise<void> {
    if (!this.isAdmin)
      return
    await this.load(async (version) => {
      const [names, instances, health] = await Promise.all([
        this.request(() => managementApi.spacegate.config.names({}), version, true),
        this.request(() => managementApi.spacegate.discovery.instances({}), version, true),
        this.request(() => managementApi.spacegate.discovery.health({}), version, true),
      ])
      if (version !== this.requestVersion)
        return
      if (names?.success) {
        this.gateways = names.data.items
        this.applyVersion(names.data.version)
        if (!this.selectedGateway || !names.data.items.includes(this.selectedGateway))
          this.selectedGateway = names.data.items[0] ?? ''
      }
      if (instances?.success) {
        this.instances = instances.data.items
        this.applyVersion(instances.data.version)
      }
      if (health?.success) {
        this.health = health.data.health
        this.applyVersion(health.data.version)
      }
    })
    if (this.selectedGateway && this.routes.length === 0)
      await this.loadRoutes(this.selectedGateway)
  }

  /** 网关范围重载；上游把该 POST 视为写操作，因此使用当前快照版本。 */
  async reloadGateway(instance: string, gateway = this.selectedGateway): Promise<boolean> {
    if (!this.isAdmin || !gateway) {
      if (!gateway)
        this.error = m.spacegate_gateway_required()
      return false
    }
    return this.reloadScope(() => managementApi.spacegate.discovery.reloadGateway({ instance, gateway, version: this.version }))
  }

  /** 全局重载。 */
  async reloadGlobal(instance: string): Promise<boolean> {
    if (!this.isAdmin)
      return false
    return this.reloadScope(() => managementApi.spacegate.discovery.reloadGlobal({ instance, version: this.version }))
  }

  /** 路由范围重载。 */
  async reloadRoute(instance: string, route = this.selectedRoute, gateway = this.selectedGateway): Promise<boolean> {
    if (!this.isAdmin || !gateway || !route) {
      if (!gateway)
        this.error = m.spacegate_gateway_required()
      else if (!route)
        this.error = m.spacegate_route_required()
      return false
    }
    return this.reloadScope(() => managementApi.spacegate.discovery.reloadRoute({ instance, gateway, route, version: this.version }))
  }

  /**
   * 冲突后的显式刷新。只由用户点击横幅触发，不自动重试冲突写。
   * 重新读取当前页面数据并向页面发出 reloadEpoch，让仍打开的编辑器关闭过期草稿。
   */
  async reloadAfterConflict(): Promise<void> {
    this.conflict = false
    this.conflictServerVersion = null
    this.error = ''
    this.reloadEpoch++
    if (!this.isAdmin)
      return
    if (this.page === 'overview') {
      await this.loadOverview()
}
    else if (this.page === 'gateways') {
      await this.loadGateways()
      if (this.selectedGateway)
        await this.loadGateway()
    }
    else if (this.page === 'routes') {
      await this.loadGateways()
      await this.loadRoutes()
    }
    else if (this.page === 'plugins') {
      await this.loadPlugins(this.selectedPluginCode || undefined)
}
    else if (this.page === 'ai-queue') {
      await this.loadAiQueue()
}
    else if (this.page === 'instances') {
      await this.loadInstances()
}
  }

  /** 重载类写操作的共同路径：写成功后更新版本并提示。 */
  private async reloadScope(task: () => Promise<HaiResult<{ version: number }>>): Promise<boolean> {
    let reloaded = false
    await this.mutate(async () => {
      const result = await this.request(task, undefined, true)
      if (!result?.success)
        return
      reloaded = true
      this.applyVersion(result.data.version)
      this.notice = m.spacegate_reload_success()
    })
    return reloaded
  }

  private applyVersion(version: number): void {
    if (version > this.version)
      this.version = version
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
    this.gateways = []
    this.gatewaySummaries = []
    this.gateway = null
    this.gatewayConfig = null
    this.gatewayEditVersion = 0
    this.routeEditVersion = 0
    this.pluginEditVersion = 0
    this.routes = []
    this.route = null
    this.plugins = []
    this.plugin = null
    this.instances = []
    this.health = {}
    this.aiRules = []
    this.aiResolution = null
    this.conflict = false
    this.conflictServerVersion = null
  }

  private expireSession(): void {
    if (!this.user)
      return
    this.clearSession()
    this.error = m.session_expired()
    void managementApi.auth.clear()
  }

  private async request<T>(task: () => Promise<HaiResult<T>>, version?: number, spacegate = false): Promise<HaiResult<T> | undefined> {
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
              this.gateways = []
              this.gatewaySummaries = []
              this.gateway = null
              this.gatewayConfig = null
              this.routes = []
              this.route = null
              this.plugins = []
              this.plugin = null
              this.instances = []
              this.health = {}
              this.aiRules = []
              this.aiResolution = null
              this.page = 'settings'
            }
          }
          this.error = m.forbidden()
        }
        else if (result.error.httpStatus === 409 && spacegate) {
          // 冲突不重试、不自动刷新编辑草稿；页面必须通过横幅显式刷新。
          this.conflict = true
          this.conflictServerVersion = parseServerVersion(result.error.message)
          this.error = m.version_conflict({ version: String(this.conflictServerVersion ?? '?') })
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
