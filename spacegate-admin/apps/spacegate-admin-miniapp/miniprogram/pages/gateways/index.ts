import { spacegate } from '../../lib/api'
import { gatewayAt, loadGatewaySelection } from '../../lib/gateways'
import { labelsFor, t } from '../../lib/i18n'
import { backToOverview, ensureSession, setPageTitle } from '../../lib/nav'
import { setCurrentGateway } from '../../lib/state'
import { confirmDelete, datasetString, describeError, parseJsonObject, prettyJson, toastDone } from '../../lib/ui'

const PAGE_LABELS = [
  'mini_nav_gateways',
  'mini_back',
  'mini_section_gateway',
  'mini_label_name',
  'mini_label_json',
  'mini_label_version',
  'mini_button_create',
  'mini_button_save',
  'mini_button_delete',
  'mini_button_refresh',
  'mini_button_edit',
  'mini_danger_zone',
  'mini_empty',
  'mini_select_gateway',
  'mini_required',
  'mini_loading',
] as const

/** 新建网关时的最小合法 JSON 对象；仍由服务端与上游做最终校验。 */
const EMPTY_GATEWAY_JSON = '{\n  "listeners": [],\n  "backends": []\n}'

Page({
  data: {
    labels: labelsFor(PAGE_LABELS),
    gatewayNames: [] as string[],
    gatewayIndex: 0,
    currentGateway: '',
    editingName: '',
    name: '',
    gatewayJson: EMPTY_GATEWAY_JSON,
    version: 0,
    createMode: true,
    loading: false,
    saving: false,
    error: '',
  },
  onShow() {
    if (!ensureSession())
      return
    setPageTitle(t('mini_nav_gateways'))
    this.setData({ labels: labelsFor(PAGE_LABELS) })
    void this.load()
  },
  async load() {
    this.setData({ loading: true, error: '' })
    try {
      const selection = await loadGatewaySelection()
      if (!selection.current) {
        this.setData({
          gatewayNames: [],
          gatewayIndex: 0,
          currentGateway: '',
          editingName: '',
          name: '',
          gatewayJson: EMPTY_GATEWAY_JSON,
          version: selection.version,
          createMode: true,
        })
        return
      }
      this.setData({
        gatewayNames: selection.names,
        gatewayIndex: Math.max(0, selection.names.indexOf(selection.current)),
        currentGateway: selection.current,
      })
      await this.loadGateway(selection.current)
    }
    catch (error) {
      this.setData({ error: describeError(error) })
    }
    finally {
      this.setData({ loading: false })
    }
  },
  async loadGateway(name: string) {
    const detail = await spacegate.gateway.get(name)
    setCurrentGateway(name)
    this.setData({
      currentGateway: name,
      editingName: name,
      name,
      gatewayJson: prettyJson(detail.gateway ?? {}),
      version: detail.version,
      createMode: false,
      error: '',
    })
  },
  async onSelectGateway(event: WxTapEvent) {
    const name = datasetString(event, 'name')
    if (!name)
      return
    try {
      await this.loadGateway(name)
    }
    catch (error) {
      this.setData({ error: describeError(error) })
    }
  },
  onGatewayChange(event: WxPickerEvent) {
    const index = Number(event.detail.value)
    const name = gatewayAt(this.data.gatewayNames, index)
    if (!name)
      return
    this.setData({ gatewayIndex: index })
    void this.loadGateway(name).catch((error: unknown) => this.setData({ error: describeError(error) }))
  },
  onStartCreate() {
    this.setData({ editingName: '', name: '', gatewayJson: EMPTY_GATEWAY_JSON, createMode: true, error: '' })
  },
  onNameInput(event: WxInputEvent) {
    this.setData({ name: event.detail.value })
  },
  onJsonInput(event: WxInputEvent) {
    this.setData({ gatewayJson: event.detail.value })
  },
  async onSave() {
    if (this.data.saving)
      return
    const name = this.data.name.trim()
    if (!name) {
      this.setData({ error: this.data.labels.mini_required })
      return
    }
    let gateway: Record<string, unknown>
    try {
      gateway = parseJsonObject(this.data.gatewayJson, this.data.labels.mini_label_json)
    }
    catch (error) {
      this.setData({ error: describeError(error) })
      return
    }
    this.setData({ saving: true, error: '' })
    try {
      const result = await spacegate.gateway.upsert({
        name,
        gateway,
        version: this.data.version,
        create: this.data.createMode || name !== this.data.editingName,
      })
      this.setData({ version: result.version, editingName: name, createMode: false })
      toastDone()
      await this.load()
    }
    catch (error) {
      const message = describeError(error)
      // 409 后必须重新读取，禁止自动重试写请求。
      await this.load()
      this.setData({ error: message })
    }
    finally {
      this.setData({ saving: false })
    }
  },
  async onDelete() {
    const name = this.data.editingName
    if (!name)
      return
    if (!await confirmDelete(name))
      return
    this.setData({ saving: true, error: '' })
    try {
      const result = await spacegate.gateway.delete({ name, version: this.data.version })
      this.setData({ version: result.version })
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
