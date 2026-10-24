import { isRecord, spacegate } from '../../lib/api'
import { labelsFor, t } from '../../lib/i18n'
import { backToOverview, ensureSession, setPageTitle } from '../../lib/nav'
import { confirmDelete, datasetIndex, describeError, FormError, parseJsonObject, prettyJson, shortJson, toastDone } from '../../lib/ui'

const PAGE_LABELS = [
  'mini_nav_ai_queue',
  'mini_back',
  'mini_section_ai_rules',
  'mini_section_ai_tools',
  'mini_label_tenant',
  'mini_label_model',
  'mini_label_path',
  'mini_label_policy',
  'mini_policy_any',
  'mini_policy_abandon',
  'mini_policy_queue',
  'mini_policy_wait',
  'mini_label_rule',
  'mini_label_extra',
  'mini_button_create',
  'mini_button_save',
  'mini_button_delete',
  'mini_button_edit',
  'mini_button_resolve',
  'mini_button_readme',
  'mini_button_rule_schema',
  'mini_button_refresh',
  'mini_rule_editing',
  'mini_rule_new',
  'mini_ai_extra_hint',
  'mini_resolve_result',
  'mini_readme',
  'mini_schema',
  'mini_empty',
  'mini_required',
  'search',
] as const

const POLICIES = [
  { value: '', key: 'mini_policy_any' },
  { value: 'abandon', key: 'mini_policy_abandon' },
  { value: 'queue', key: 'mini_policy_queue' },
  { value: 'wait', key: 'mini_policy_wait' },
] as const

interface RuleRow {
  index: number
  preview: string
  summary: string
  ruleJson: string
}

function ruleField(value: unknown, key: string): string {
  return isRecord(value) && typeof value[key] === 'string' ? value[key] : ''
}

function toRuleRow(item: unknown, index: number): RuleRow {
  const tenant = ruleField(item, 'tenant')
  const model = ruleField(item, 'model')
  const path = ruleField(item, 'path')
  const policy = ruleField(item, 'policy')
  const summary = [tenant || '-', model || '-', path || '-', policy || '-'].join(' · ')
  return { index, preview: shortJson(item), summary, ruleJson: prettyJson(item) }
}

Page({
  data: {
    labels: labelsFor(PAGE_LABELS),
    policies: POLICIES.map(item => ({ value: item.value, label: t(item.key) })),
    policyIndex: 0,
    pluginName: '',
    filterTenant: '',
    filterModel: '',
    filterPath: '',
    rules: [] as RuleRow[],
    editingIndex: -1,
    ruleJson: '{}',
    tenant: '',
    model: '',
    path: '',
    extraJson: '',
    resolveJson: '',
    readme: '',
    schemaJson: '',
    loading: false,
    saving: false,
    error: '',
  },
  onShow() {
    if (!ensureSession())
      return
    setPageTitle(t('mini_nav_ai_queue'))
    this.setData({ labels: labelsFor(PAGE_LABELS), policies: POLICIES.map(item => ({ value: item.value, label: t(item.key) })) })
    void this.loadRules()
  },
  onPluginInput(event: WxInputEvent) {
    this.setData({ pluginName: event.detail.value })
  },
  onFilterTenantInput(event: WxInputEvent) {
    this.setData({ filterTenant: event.detail.value })
  },
  onFilterModelInput(event: WxInputEvent) {
    this.setData({ filterModel: event.detail.value })
  },
  onFilterPathInput(event: WxInputEvent) {
    this.setData({ filterPath: event.detail.value })
  },
  onFilterPolicyChange(event: WxPickerEvent) {
    this.setData({ policyIndex: Number(event.detail.value) })
  },
  policyValue(index: number): string {
    return POLICIES[index]?.value ?? ''
  },
  filterPolicy(): 'abandon' | 'queue' | 'wait' | undefined {
    const value = this.policyValue(this.data.policyIndex)
    return value === 'abandon' || value === 'queue' || value === 'wait' ? value : undefined
  },
  async loadRules() {
    this.setData({ loading: true, error: '' })
    try {
      const filters: { tenant?: string, model?: string, path?: string, policy?: 'abandon' | 'queue' | 'wait' } = {}
      if (this.data.filterTenant.trim())
        filters.tenant = this.data.filterTenant.trim()
      if (this.data.filterModel.trim())
        filters.model = this.data.filterModel.trim()
      if (this.data.filterPath.trim())
        filters.path = this.data.filterPath.trim()
      const policy = this.filterPolicy()
      if (policy)
        filters.policy = policy
      const result = await spacegate.aiQueue.rules({ filters })
      this.setData({ rules: result.items.map(toRuleRow) })
    }
    catch (error) {
      this.setData({ error: describeError(error) })
    }
    finally {
      this.setData({ loading: false })
    }
  },
  onSelectRule(event: WxTapEvent) {
    const index = datasetIndex(event, 'index')
    const row = this.data.rules[index]
    if (!row)
      return
    try {
      this.applyRule(index, JSON.parse(row.ruleJson) as unknown)
    }
    catch {
      this.setData({ error: this.data.labels.mini_empty })
    }
  },
  applyRule(index: number, rule: unknown) {
    const tenant = ruleField(rule, 'tenant')
    const model = ruleField(rule, 'model')
    const path = ruleField(rule, 'path')
    const policy = ruleField(rule, 'policy')
    const policyIndex = POLICIES.findIndex(item => item.value === policy)
    this.setData({
      editingIndex: index,
      ruleJson: prettyJson(rule),
      tenant,
      model,
      path,
      policyIndex: policyIndex < 0 ? 0 : policyIndex,
      extraJson: '',
      error: '',
    })
  },
  onStartCreate() {
    this.setData({ editingIndex: -1, ruleJson: '{}', tenant: '', model: '', path: '', policyIndex: 0, extraJson: '', error: '' })
  },
  onTenantInput(event: WxInputEvent) {
    this.setData({ tenant: event.detail.value })
  },
  onModelInput(event: WxInputEvent) {
    this.setData({ model: event.detail.value })
  },
  onPathInput(event: WxInputEvent) {
    this.setData({ path: event.detail.value })
  },
  onPolicyChange(event: WxPickerEvent) {
    this.setData({ policyIndex: Number(event.detail.value) })
  },
  onRuleJsonInput(event: WxInputEvent) {
    this.setData({ ruleJson: event.detail.value })
  },
  onExtraInput(event: WxInputEvent) {
    this.setData({ extraJson: event.detail.value })
  },
  buildRule(): Record<string, unknown> {
    const base = parseJsonObject(this.data.ruleJson.trim() || '{}', this.data.labels.mini_label_rule)
    const extra = this.data.extraJson.trim()
      ? parseJsonObject(this.data.extraJson, this.data.labels.mini_label_extra)
      : {}
    const tenant = this.data.tenant.trim()
    if (!tenant)
      throw new FormError(this.data.labels.mini_required)
    const policy = this.policyValue(this.data.policyIndex)
    return {
      ...base,
      ...extra,
      tenant,
      ...(this.data.model.trim() ? { model: this.data.model.trim() } : {}),
      ...(this.data.path.trim() ? { path: this.data.path.trim() } : {}),
      ...(policy ? { policy } : {}),
    }
  },
  async onSaveRule() {
    if (this.data.saving)
      return
    let rule: Record<string, unknown>
    try {
      rule = this.buildRule()
    }
    catch (error) {
      this.setData({ error: describeError(error) })
      return
    }
    this.setData({ saving: true, error: '' })
    try {
      const result = await spacegate.aiQueue.upsert({ rule })
      this.setData({ ruleJson: prettyJson(result.rule) })
      toastDone()
      await this.loadRules()
    }
    catch (error) {
      this.setData({ error: describeError(error) })
    }
    finally {
      this.setData({ saving: false })
    }
  },
  async onDeleteRule() {
    if (this.data.saving)
      return
    let rule: Record<string, unknown>
    try {
      rule = this.buildRule()
    }
    catch (error) {
      this.setData({ error: describeError(error) })
      return
    }
    if (!await confirmDelete(ruleField(rule, 'tenant') || 'rule'))
      return
    this.setData({ saving: true, error: '' })
    try {
      const result = await spacegate.aiQueue.delete({ rule })
      toastDone(t('mini_operation_done'))
      if (result.deleted < 1)
        this.setData({ error: t('mini_empty') })
      this.setData({ editingIndex: -1, ruleJson: '{}', tenant: '', model: '', path: '', policyIndex: 0, extraJson: '' })
      await this.loadRules()
    }
    catch (error) {
      this.setData({ error: describeError(error) })
    }
    finally {
      this.setData({ saving: false })
    }
  },
  async onResolve() {
    const tenant = this.data.filterTenant.trim() || this.data.tenant.trim()
    if (!tenant) {
      this.setData({ error: this.data.labels.mini_required })
      return
    }
    try {
      const policy = this.filterPolicy()
      const result = await spacegate.aiQueue.resolve({
        tenant,
        ...(this.data.filterModel.trim() ? { model: this.data.filterModel.trim() } : {}),
        ...(this.data.filterPath.trim() ? { path: this.data.filterPath.trim() } : {}),
        ...(policy ? { policy } : {}),
      })
      this.setData({ resolveJson: prettyJson(result.resolution), error: '' })
    }
    catch (error) {
      this.setData({ error: describeError(error) })
    }
  },
  async onReadme() {
    try {
      const plugin = this.data.pluginName.trim()
      const result = await spacegate.aiQueue.readme(plugin || undefined)
      this.setData({ readme: result.markdown, error: '' })
    }
    catch (error) {
      this.setData({ error: describeError(error) })
    }
  },
  async onSchema() {
    try {
      const plugin = this.data.pluginName.trim()
      const result = await spacegate.aiQueue.schema(plugin || undefined)
      this.setData({ schemaJson: prettyJson(result.schema), error: '' })
    }
    catch (error) {
      this.setData({ error: describeError(error) })
    }
  },
  onBack() {
    backToOverview()
  },
})
