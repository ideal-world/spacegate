import { spacegate } from '../../lib/api'
import { gatewayAt, loadGatewaySelection } from '../../lib/gateways'
import { labelsFor, t } from '../../lib/i18n'
import { backToOverview, ensureSession, setPageTitle } from '../../lib/nav'
import { setCurrentGateway } from '../../lib/state'
import { confirmDelete, datasetIndex, describeError, parseJsonArray, parseJsonObject, prettyJson, shortJson, toastDone } from '../../lib/ui'

const PAGE_LABELS = [
  'mini_nav_routes',
  'mini_back',
  'mini_section_routes',
  'mini_label_route',
  'mini_label_new_name',
  'mini_label_json',
  'mini_label_plugins',
  'mini_label_version',
  'mini_button_create',
  'mini_button_save',
  'mini_button_delete',
  'mini_button_rename',
  'mini_button_edit',
  'mini_button_refresh',
  'mini_empty',
  'mini_select_gateway',
  'mini_required',
  'mini_route_plugins_hint',
  'mini_route_rename_hint',
] as const

const EMPTY_ROUTE_JSON = '{\n  "host": "",\n  "path": "/",\n  "plugins": []\n}'

interface RouteRow {
  name: string
  preview: string
}

Page({
  data: {
    labels: labelsFor(PAGE_LABELS),
    gatewayNames: [] as string[],
    gatewayIndex: 0,
    gateway: '',
    version: 0,
    routeRows: [] as RouteRow[],
    routeName: '',
    routeJson: EMPTY_ROUTE_JSON,
    pluginsJson: '',
    renameName: '',
    editingName: '',
    createMode: true,
    loading: false,
    saving: false,
    error: '',
  },
  onShow() {
    if (!ensureSession())
      return
    setPageTitle(t('mini_nav_routes'))
    this.setData({ labels: labelsFor(PAGE_LABELS) })
    void this.load()
  },
  async load() {
    this.setData({ loading: true, error: '' })
    try {
      const selection = await loadGatewaySelection()
      if (!selection.current) {
        this.setData({ gatewayNames: [], gateway: '', routeRows: [], version: selection.version })
        return
      }
      const list = await spacegate.route.list({ gateway: selection.current })
      this.setData({
        gatewayNames: selection.names,
        gatewayIndex: Math.max(0, selection.names.indexOf(selection.current)),
        gateway: selection.current,
        routeRows: list.items.map(item => ({ name: item.name, preview: shortJson(item.route) })),
        version: list.version,
      })
    }
    catch (error) {
      this.setData({ error: describeError(error) })
    }
    finally {
      this.setData({ loading: false })
    }
  },
  onGatewayChange(event: WxPickerEvent) {
    const index = Number(event.detail.value)
    const name = gatewayAt(this.data.gatewayNames, index)
    if (!name)
      return
    setCurrentGateway(name)
    this.setData({ gatewayIndex: index, gateway: name, editingName: '', createMode: true, routeName: '', routeJson: EMPTY_ROUTE_JSON, pluginsJson: '', renameName: '' })
    void this.load()
  },
  async onSelectRoute(event: WxTapEvent) {
    const index = datasetIndex(event, 'index')
    const row = this.data.routeRows[index]
    if (!row || !this.data.gateway)
      return
    try {
      const detail = await spacegate.route.get(this.data.gateway, row.name)
      const route = detail.route
      const plugins = route && typeof route === 'object' && !Array.isArray(route)
        ? (route as Record<string, unknown>).plugins
        : undefined
      this.setData({
        editingName: row.name,
        routeName: row.name,
        routeJson: prettyJson(route ?? {}),
        pluginsJson: Array.isArray(plugins) ? prettyJson(plugins) : '',
        renameName: '',
        version: detail.version,
        createMode: false,
        error: '',
      })
    }
    catch (error) {
      this.setData({ error: describeError(error) })
    }
  },
  onStartCreate() {
    this.setData({ editingName: '', routeName: '', routeJson: EMPTY_ROUTE_JSON, pluginsJson: '', renameName: '', createMode: true, error: '' })
  },
  onRouteNameInput(event: WxInputEvent) {
    this.setData({ routeName: event.detail.value })
  },
  onRouteJsonInput(event: WxInputEvent) {
    this.setData({ routeJson: event.detail.value })
  },
  onPluginsInput(event: WxInputEvent) {
    this.setData({ pluginsJson: event.detail.value })
  },
  onRenameInput(event: WxInputEvent) {
    this.setData({ renameName: event.detail.value })
  },
  buildConfig(): Record<string, unknown> {
    const config = parseJsonObject(this.data.routeJson, this.data.labels.mini_label_json)
    if (this.data.pluginsJson.trim()) {
      const plugins = parseJsonArray(this.data.pluginsJson, this.data.labels.mini_label_plugins)
      config.plugins = plugins
    }
    return config
  },
  async onSave() {
    if (this.data.saving)
      return
    const gateway = this.data.gateway
    const route = this.data.routeName.trim()
    if (!gateway) {
      this.setData({ error: this.data.labels.mini_select_gateway })
      return
    }
    if (!route) {
      this.setData({ error: this.data.labels.mini_required })
      return
    }
    let config: Record<string, unknown>
    try {
      config = this.buildConfig()
    }
    catch (error) {
      this.setData({ error: describeError(error) })
      return
    }
    this.setData({ saving: true, error: '' })
    try {
      const result = await spacegate.route.upsert({
        gateway,
        route,
        config,
        version: this.data.version,
        create: this.data.createMode || route !== this.data.editingName,
      })
      this.setData({ version: result.version, editingName: route, createMode: false })
      toastDone()
      await this.load()
    }
    catch (error) {
      const message = describeError(error)
      await this.load()
      this.setData({ error: message })
    }
    finally {
      this.setData({ saving: false })
    }
  },
  async onRename() {
    if (this.data.saving || !this.data.editingName)
      return
    const newName = this.data.renameName.trim()
    if (!newName) {
      this.setData({ error: this.data.labels.mini_required })
      return
    }
    let config: Record<string, unknown>
    try {
      config = this.buildConfig()
    }
    catch (error) {
      this.setData({ error: describeError(error) })
      return
    }
    this.setData({ saving: true, error: '' })
    try {
      const result = await spacegate.route.rename({
        gateway: this.data.gateway,
        route: this.data.editingName,
        newName,
        config,
        version: this.data.version,
      })
      this.setData({ version: result.version, editingName: newName, routeName: newName, renameName: '' })
      toastDone()
      await this.load()
    }
    catch (error) {
      const message = describeError(error)
      await this.load()
      this.setData({ error: message })
    }
    finally {
      this.setData({ saving: false })
    }
  },
  async onDelete() {
    if (this.data.saving || !this.data.editingName)
      return
    if (!await confirmDelete(this.data.editingName))
      return
    this.setData({ saving: true, error: '' })
    try {
      const result = await spacegate.route.delete({ gateway: this.data.gateway, route: this.data.editingName, version: this.data.version })
      this.setData({ version: result.version, editingName: '', createMode: true, routeName: '', routeJson: EMPTY_ROUTE_JSON, pluginsJson: '' })
      toastDone()
      await this.load()
    }
    catch (error) {
      const message = describeError(error)
      await this.load()
      this.setData({ error: message })
    }
    finally {
      this.setData({ saving: false })
    }
  },
  onRefresh() {
    void this.load()
  },
  onBack() {
    backToOverview()
  },
})
