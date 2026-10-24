import { spacegate } from '../../lib/api'
import { gatewayAt, loadGatewaySelection } from '../../lib/gateways'
import { labelsFor, t } from '../../lib/i18n'
import { backToOverview, ensureSession, setPageTitle } from '../../lib/nav'
import { setCurrentGateway } from '../../lib/state'
import { datasetString, describeError, toastDone } from '../../lib/ui'

const PAGE_LABELS = [
  'mini_nav_instances',
  'mini_back',
  'mini_section_health',
  'mini_section_reload',
  'mini_label_instance',
  'mini_label_route',
  'mini_label_version',
  'mini_status_online',
  'mini_status_offline',
  'mini_button_reload_gateway',
  'mini_button_reload_global',
  'mini_button_reload_route',
  'mini_button_refresh',
  'mini_empty',
  'mini_select_gateway',
  'mini_required',
  'mini_reload_route_hint',
] as const

interface InstanceRow {
  name: string
  online: boolean
  status: string
}

Page({
  data: {
    labels: labelsFor(PAGE_LABELS),
    instances: [] as InstanceRow[],
    gatewayNames: [] as string[],
    gatewayIndex: 0,
    gateway: '',
    routeName: '',
    version: 0,
    loading: false,
    reloading: '',
    error: '',
  },
  onShow() {
    if (!ensureSession())
      return
    setPageTitle(t('mini_nav_instances'))
    this.setData({ labels: labelsFor(PAGE_LABELS) })
    void this.load()
  },
  async load() {
    this.setData({ loading: true, error: '' })
    try {
      const selection = await loadGatewaySelection()
      const [instances, health] = await Promise.all([
        spacegate.discovery.instances(),
        spacegate.discovery.health(),
      ])
      this.setData({
        gatewayNames: selection.names,
        gatewayIndex: Math.max(0, selection.names.indexOf(selection.current)),
        gateway: selection.current,
        version: instances.version,
        instances: instances.items.map(name => ({
          name,
          online: health.health[name] === true,
          status: health.health[name] === true ? t('mini_status_online') : t('mini_status_offline'),
        })),
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
    this.setData({ gatewayIndex: index, gateway: name })
  },
  onRouteInput(event: WxInputEvent) {
    this.setData({ routeName: event.detail.value })
  },
  async onReloadGateway(event: WxTapEvent) {
    const instance = datasetString(event, 'name')
    if (!instance || !this.data.gateway) {
      this.setData({ error: this.data.labels.mini_select_gateway })
      return
    }
    await this.reload('gateway', instance, () => spacegate.discovery.reloadGateway({ instance, gateway: this.data.gateway, version: this.data.version }))
  },
  async onReloadGlobal(event: WxTapEvent) {
    const instance = datasetString(event, 'name')
    if (!instance) {
      this.setData({ error: this.data.labels.mini_required })
      return
    }
    await this.reload('global', instance, () => spacegate.discovery.reloadGlobal({ instance, version: this.data.version }))
  },
  async onReloadRoute(event: WxTapEvent) {
    const instance = datasetString(event, 'name')
    const route = this.data.routeName.trim()
    if (!instance || !this.data.gateway || !route) {
      this.setData({ error: this.data.labels.mini_required })
      return
    }
    await this.reload('route', instance, () => spacegate.discovery.reloadRoute({ instance, gateway: this.data.gateway, route, version: this.data.version }))
  },
  async reload(kind: string, instance: string, action: () => Promise<{ version: number }>) {
    if (this.data.reloading)
      return
    this.setData({ reloading: `${kind}:${instance}`, error: '' })
    try {
      const result = await action()
      this.setData({ version: result.version })
      toastDone()
      await this.load()
    }
    catch (error) {
      const message = describeError(error)
      // 重载也是写操作，会推进版本；冲突后必须重新读取。
      await this.load()
      this.setData({ error: message })
    }
    finally {
      this.setData({ reloading: '' })
    }
  },
  onRefresh() {
    void this.load()
  },
  onBack() {
    backToOverview()
  },
})
