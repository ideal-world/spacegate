import { spacegate } from '../../lib/api'
import { gatewayAt, loadGatewaySelection } from '../../lib/gateways'
import { labelsFor, t } from '../../lib/i18n'
import { ensureSession, openPage, PAGE_PATHS, setPageTitle } from '../../lib/nav'
import { setCurrentGateway } from '../../lib/state'
import { datasetString, describeError } from '../../lib/ui'

const PAGE_LABELS = [
  'mini_app_name',
  'mini_nav_overview',
  'mini_nav_gateways',
  'mini_nav_routes',
  'mini_nav_plugins',
  'mini_nav_ai_queue',
  'mini_nav_instances',
  'mini_nav_profile',
  'mini_current_gateway',
  'mini_no_gateway',
  'mini_gateway_count',
  'mini_route_count',
  'mini_plugin_count',
  'mini_instance_online',
  'mini_instance_total',
  'mini_health_summary',
  'mini_section_health',
  'mini_overview_manage',
  'home_refresh',
  'mini_loading',
  'mini_empty',
  'mini_operation_done',
  'mini_status_online',
  'mini_status_offline',
] as const

/** 概览模块入口；与 app.json 页面一一对应。 */
const NAV_ITEMS = [
  { page: 'gateways', labelKey: 'mini_nav_gateways' },
  { page: 'routes', labelKey: 'mini_nav_routes' },
  { page: 'plugins', labelKey: 'mini_nav_plugins' },
  { page: 'aiQueue', labelKey: 'mini_nav_ai_queue' },
  { page: 'instances', labelKey: 'mini_nav_instances' },
  { page: 'profile', labelKey: 'mini_nav_profile' },
] as const

Page({
  data: {
    labels: labelsFor(PAGE_LABELS),
    navItems: NAV_ITEMS.map(item => ({ page: item.page, title: t(item.labelKey) })),
    gatewayNames: [] as string[],
    gatewayIndex: 0,
    gatewayName: '',
    routeCount: 0,
    pluginCount: 0,
    onlineCount: 0,
    instanceTotal: 0,
    gatewayExists: false,
    loading: false,
    error: '',
  },
  onShow() {
    if (!ensureSession())
      return
    setPageTitle(t('mini_nav_overview'))
    this.setData({ labels: labelsFor(PAGE_LABELS), navItems: NAV_ITEMS.map(item => ({ page: item.page, title: t(item.labelKey) })) })
    void this.load()
  },
  async load() {
    if (this.data.loading)
      return
    this.setData({ loading: true, error: '' })
    try {
      const selection = await loadGatewaySelection()
      const gatewayName = selection.current
      const [gateway, routes, plugins, instances, health] = await Promise.all([
        gatewayName ? spacegate.gateway.get(gatewayName) : Promise.resolve({ gateway: null, version: selection.version }),
        gatewayName ? spacegate.route.list({ gateway: gatewayName }) : Promise.resolve({ items: [], version: selection.version }),
        spacegate.plugin.list({}),
        spacegate.discovery.instances(),
        spacegate.discovery.health(),
      ])
      const onlineCount = Object.values(health.health).filter(Boolean).length
      this.setData({
        gatewayNames: selection.names,
        gatewayIndex: Math.max(0, selection.names.indexOf(gatewayName)),
        gatewayName,
        gatewayExists: gateway.gateway !== null && gateway.gateway !== undefined,
        routeCount: routes.items.length,
        pluginCount: plugins.items.length,
        onlineCount,
        instanceTotal: instances.items.length,
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
    this.setData({ gatewayIndex: index, gatewayName: name })
    void this.load()
  },
  onRefresh() {
    void this.load()
  },
  onManageGateways() {
    openPage(PAGE_PATHS.gateways)
  },
  onOpenModule(event: WxTapEvent) {
    const page = datasetString(event, 'page')
    const target = PAGE_PATHS[page as keyof typeof PAGE_PATHS]
    if (target)
      openPage(target)
  },
})
