import type { PluginInstanceId, SpacegatePluginUpsertInput } from 'spacegate-admin-contract'
import { isRecord, spacegate } from '../../lib/api'
import { labelsFor, t } from '../../lib/i18n'
import { backToOverview, ensureSession, setPageTitle } from '../../lib/nav'
import { confirmDelete, datasetIndex, describeError, FormError, parseJsonObject, parseJsonValue, prettyJson, shortJson, toastDone } from '../../lib/ui'

const PAGE_LABELS = [
  'mini_nav_plugins',
  'mini_back',
  'mini_section_plugin_editor',
  'mini_section_plugin_tools',
  'mini_label_code',
  'mini_label_kind',
  'mini_kind_named',
  'mini_kind_anon',
  'mini_label_name',
  'mini_label_uid',
  'mini_label_display_name',
  'mini_label_spec',
  'mini_label_version',
  'mini_search_plugin_code',
  'mini_button_create',
  'mini_button_save',
  'mini_button_delete',
  'mini_button_edit',
  'mini_button_refresh',
  'mini_button_view_schema',
  'mini_button_wasm_schema',
  'mini_button_preview',
  'mini_label_image_url',
  'mini_label_schema_path',
  'mini_label_http_headers',
  'mini_plugin_filter_placeholder',
  'mini_spec_hint',
  'mini_no_wasm_schema',
  'mini_empty',
  'mini_required',
  'mini_schema',
] as const

const KINDS: PluginInstanceId['kind'][] = ['named', 'anon']

interface PluginRow {
  index: number
  label: string
  displayName: string
  preview: string
  idJson: string
  code: string
  kind: PluginInstanceId['kind']
  name: string
  uid: string
  specJson: string
}

function toPluginRow(item: unknown, index: number): PluginRow {
  const record = isRecord(item) ? item : {}
  const rawId = isRecord(record.id) ? record.id : {}
  const kind: PluginInstanceId['kind'] = rawId.kind === 'anon' ? 'anon' : 'named'
  const code = typeof rawId.code === 'string' ? rawId.code : ''
  const name = typeof rawId.name === 'string' ? rawId.name : ''
  const uid = typeof rawId.uid === 'string' ? rawId.uid : ''
  const displayName = typeof record.displayName === 'string' ? record.displayName : ''
  const id: PluginInstanceId = kind === 'anon' ? { code, kind, uid } : { code, kind, name }
  return {
    index,
    label: `${code} · ${kind === 'anon' ? uid : name}`,
    displayName,
    preview: shortJson(record.spec),
    idJson: JSON.stringify(id),
    code,
    kind,
    name,
    uid,
    specJson: prettyJson(record.spec ?? {}),
  }
}

Page({
  data: {
    labels: labelsFor(PAGE_LABELS),
    kinds: ['named', 'anon'].map(kind => ({ kind, label: t(kind === 'named' ? 'mini_kind_named' : 'mini_kind_anon') })) as { kind: PluginInstanceId['kind'], label: string }[],
    kindIndex: 0,
    filterCode: '',
    version: 0,
    pluginRows: [] as PluginRow[],
    catalog: [] as string[],
    editingIdJson: '',
    code: '',
    name: '',
    uid: '',
    displayName: '',
    specJson: '{}',
    createMode: true,
    schemaJson: '',
    wasmJson: '',
    imageUrl: '',
    schemaPath: '',
    headersJson: '',
    ociAuthJson: '',
    previewJson: '',
    loading: false,
    saving: false,
    error: '',
  },
  onShow() {
    if (!ensureSession())
      return
    setPageTitle(t('mini_nav_plugins'))
    this.setData({ labels: labelsFor(PAGE_LABELS) })
    void this.load()
  },
  async load() {
    this.setData({ loading: true, error: '' })
    try {
      const code = this.data.filterCode.trim()
      const [list, catalog] = await Promise.all([
        spacegate.plugin.list(code ? { code } : {}),
        spacegate.plugin.catalog(),
      ])
      this.setData({
        version: list.version,
        pluginRows: list.items.map(toPluginRow),
        catalog: [...catalog.items],
      })
    }
    catch (error) {
      this.setData({ error: describeError(error) })
    }
    finally {
      this.setData({ loading: false })
    }
  },
  onFilterInput(event: WxInputEvent) {
    this.setData({ filterCode: event.detail.value })
  },
  onKindChange(event: WxPickerEvent) {
    this.setData({ kindIndex: Number(event.detail.value) })
  },
  onCodeInput(event: WxInputEvent) {
    this.setData({ code: event.detail.value })
  },
  onNameInput(event: WxInputEvent) {
    this.setData({ name: event.detail.value })
  },
  onUidInput(event: WxInputEvent) {
    this.setData({ uid: event.detail.value })
  },
  onDisplayNameInput(event: WxInputEvent) {
    this.setData({ displayName: event.detail.value })
  },
  onSpecInput(event: WxInputEvent) {
    this.setData({ specJson: event.detail.value })
  },
  onCatalogTap(event: WxTapEvent) {
    const index = datasetIndex(event, 'index')
    const code = this.data.catalog[index]
    if (code)
      this.setData({ code })
  },
  onSelectPlugin(event: WxTapEvent) {
    const index = datasetIndex(event, 'index')
    const row = this.data.pluginRows[index]
    if (!row)
      return
    this.setData({
      editingIdJson: row.idJson,
      code: row.code,
      kindIndex: KINDS.indexOf(row.kind),
      name: row.name,
      uid: row.uid,
      displayName: row.displayName,
      specJson: row.specJson,
      createMode: false,
      error: '',
    })
  },
  onStartCreate() {
    this.setData({ editingIdJson: '', code: '', kindIndex: 0, name: '', uid: '', displayName: '', specJson: '{}', createMode: true, error: '' })
  },
  buildId(): PluginInstanceId {
    const code = this.data.code.trim()
    if (!code)
      throw new FormError(this.data.labels.mini_required)
    const kind = KINDS[this.data.kindIndex] ?? 'named'
    if (kind === 'anon') {
      const uid = this.data.uid.trim()
      if (!uid)
        throw new FormError(this.data.labels.mini_required)
      return { code, kind, uid }
    }
    const name = this.data.name.trim()
    if (!name)
      throw new FormError(this.data.labels.mini_required)
    return { code, kind, name }
  },
  async onSave() {
    if (this.data.saving)
      return
    let id: PluginInstanceId
    let spec: SpacegatePluginUpsertInput['plugin']['spec']
    try {
      id = this.buildId()
      // JSON.parse 的返回值天然是 JSON 值；契约类型在提交前由服务端再次校验。
      spec = parseJsonValue(this.data.specJson, this.data.labels.mini_label_spec) as SpacegatePluginUpsertInput['plugin']['spec']
    }
    catch (error) {
      this.setData({ error: describeError(error) })
      return
    }
    this.setData({ saving: true, error: '' })
    try {
      const displayName = this.data.displayName.trim()
      const result = await spacegate.plugin.upsert({
        plugin: { id, ...(displayName ? { displayName } : {}), spec },
        version: this.data.version,
        create: this.data.createMode,
      })
      this.setData({ version: result.version, editingIdJson: JSON.stringify(id), createMode: false })
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
    if (this.data.saving || !this.data.editingIdJson)
      return
    let id: PluginInstanceId
    try {
      id = JSON.parse(this.data.editingIdJson) as PluginInstanceId
    }
    catch {
      return
    }
    if (!await confirmDelete(`${id.code} · ${id.kind}`))
      return
    this.setData({ saving: true, error: '' })
    try {
      const result = await spacegate.plugin.delete({ id, version: this.data.version })
      this.setData({ version: result.version, editingIdJson: '', createMode: true })
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
  async onSchema() {
    const code = this.data.code.trim()
    if (!code) {
      this.setData({ error: this.data.labels.mini_required })
      return
    }
    try {
      const result = await spacegate.plugin.schema(code)
      this.setData({ schemaJson: prettyJson(result.schema), error: '' })
    }
    catch (error) {
      this.setData({ error: describeError(error) })
    }
  },
  async onWasmSchema() {
    if (!this.data.editingIdJson) {
      this.setData({ error: this.data.labels.mini_required })
      return
    }
    try {
      const id = JSON.parse(this.data.editingIdJson) as PluginInstanceId
      const result = await spacegate.plugin.wasmSchema(id)
      this.setData({ wasmJson: prettyJson(result.schema), error: '' })
    }
    catch (error) {
      this.setData({ error: describeError(error) })
      this.setData({ wasmJson: this.data.labels.mini_no_wasm_schema })
    }
  },
  onImageUrlInput(event: WxInputEvent) {
    this.setData({ imageUrl: event.detail.value })
  },
  onSchemaPathInput(event: WxInputEvent) {
    this.setData({ schemaPath: event.detail.value })
  },
  onHeadersInput(event: WxInputEvent) {
    this.setData({ headersJson: event.detail.value })
  },
  onOciAuthInput(event: WxInputEvent) {
    this.setData({ ociAuthJson: event.detail.value })
  },
  async onPreview() {
    const imageUrl = this.data.imageUrl.trim()
    if (!imageUrl) {
      this.setData({ error: this.data.labels.mini_required })
      return
    }
    try {
      const headers: Record<string, string> = {}
      if (this.data.headersJson.trim()) {
        const parsed = parseJsonObject(this.data.headersJson, this.data.labels.mini_label_http_headers)
        for (const [key, value] of Object.entries(parsed)) {
          if (typeof value !== 'string')
            throw new Error(this.data.labels.mini_label_http_headers)
          headers[key] = value
        }
      }
      const ociAuth = this.data.ociAuthJson.trim() ? parseJsonObject(this.data.ociAuthJson, 'ociAuth') : undefined
      const result = await spacegate.plugin.previewWasmSchema({
        imageUrl,
        ...(this.data.schemaPath.trim() ? { schemaPath: this.data.schemaPath.trim() } : {}),
        ...(Object.keys(headers).length ? { httpHeaders: headers } : {}),
        ...(ociAuth === undefined ? {} : { ociAuth }),
      })
      this.setData({ previewJson: prettyJson(result.schema), error: '' })
    }
    catch (error) {
      this.setData({ error: describeError(error) })
    }
  },
  async onRefresh() {
    try {
      const result = await spacegate.plugin.refresh()
      this.setData({ version: result.version })
      toastDone()
      await this.load()
    }
    catch (error) {
      this.setData({ error: describeError(error) })
    }
  },
  onBack() {
    backToOverview()
  },
})
