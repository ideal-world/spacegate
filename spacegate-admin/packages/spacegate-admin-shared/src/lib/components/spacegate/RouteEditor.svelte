<script lang="ts">
  import type { ManagementStore } from '../../stores/management-store.svelte.js'
  import type { JsonRecord, PluginInstance, PluginPick, RouteType } from '../../spacegate/model.js'
  import { Accordion, Alert, Drawer, FormField, FormLayout, Tabs, TagInput } from '@h-ai/ui/components/compounds/index'
  import { Button, Checkbox, Input, Select, Switch } from '@h-ai/ui/components/primitives/index'
  import * as m from '../../paraglide/messages.js'
  import {
    asRecord,
    cloneRecord,
    createBinding,
    isMcpRoute,
    newBackend,
    newHttpRule,
    newRoute,
    readBoolean,
    readNumber,
    readRecord,
    readString,
    routeTypeOf,
  } from '../../spacegate/model.js'
  import JsonTextarea from './JsonTextarea.svelte'
  import PluginBindingEditor from './PluginBindingEditor.svelte'
  import PluginEditor from './PluginEditor.svelte'
  import PluginPicker from './PluginPicker.svelte'
  import VersionConflictBanner from './VersionConflictBanner.svelte'

  interface Props {
    store: ManagementStore
    open?: boolean
    routeName?: string | null
  }
  let { store, open = $bindable(false), routeName = null }: Props = $props()

  let draft = $state<JsonRecord>(newRoute('new-route', 'http'))
  let originalName = $state<string | null>(null)
  let createType = $state<RouteType>('http')
  let deleting = $state(false)
  // 用 undefined 表示“尚未初始化”，避免新建（目标为 null）与初始值相等而被跳过。
  let initializedFor = $state<string | null | undefined>(undefined)
  let seenEpoch = $state(0)
  // 从插件选择器“新建实例”进入时，用同一个编辑器抽屉完成创建并回填到插件链。
  let pluginEditorOpen = $state(false)
  let pickerOpen = $state(false)
  let pendingPluginBinding = $state<((instance: PluginInstance) => void) | null>(null)
  let pluginCreateCode = $state('')
  // 标签页与规则折叠状态，与旧管理端 RouteEditorDrawer 的 el-tabs / el-collapse 对应。
  let activeTab = $state('basic')
  let expandedRules = $state<string[]>(['0'])

  const isCreate = $derived(originalName === null)
  const mcp = $derived(isMcpRoute(draft))
  const routes = $derived(Array.isArray(draft.rules) ? draft.rules : [])
  const mcpBackends = $derived(Array.isArray(draft.backends) ? draft.backends : [])
  const hostnames = $derived(Array.isArray(draft.hostnames) ? draft.hostnames.filter((item): item is string => typeof item === 'string') : [])
  const routePlugins = $derived(Array.isArray(draft.plugins) ? draft.plugins : [])
  const routeType = $derived(isCreate ? createType : routeTypeOf(draft))
  const transport = $derived(readString(draft, 'transport') || 'streamable_http')
  const transportOptions = [{ value: 'streamable_http', label: 'streamable_http' }, { value: 'legacy_sse', label: 'legacy_sse' }]
  const timeoutModeOptions = [{ value: '', label: m.none() }, { value: 'disabled', label: 'disabled' }, { value: 'request', label: 'request' }]
  const balanceOptions = [{ value: '', label: m.none() }, { value: 'random', label: 'random' }, { value: 'ip_hash', label: 'ip_hash' }, { value: 'mcp_session', label: 'mcp_session' }]
  const protocolOptions = [{ value: '', label: m.none() }, { value: 'http', label: 'http' }, { value: 'https', label: 'https' }]
  const hostKindOptions = [{ value: 'Host', label: 'Host' }, { value: 'K8sService', label: 'K8sService' }, { value: 'File', label: 'File' }]
  const pathKindOptions = [{ value: 'Prefix', label: 'Prefix' }, { value: 'Exact', label: 'Exact' }, { value: 'RegExp', label: 'RegExp' }]
  const headerKindOptions = [{ value: 'exact', label: 'exact' }, { value: 'reg_exp', label: 'reg_exp' }]
  const queryKindOptions = [{ value: 'exact', label: 'exact' }, { value: 'regular', label: 'regular' }]
  const methodOptions = ['GET', 'POST', 'PUT', 'DELETE', 'PATCH', 'HEAD', 'OPTIONS']
  const tabItems = $derived([
    { key: 'basic', label: m.route_tab_basic() },
    ...(mcp
      ? [{ key: 'transport', label: m.route_tab_transport() }]
      : [
          { key: 'match', label: m.route_tab_match() },
          { key: 'backend', label: m.route_tab_backend() },
        ]),
    { key: 'plugins', label: m.route_tab_plugins() },
    { key: 'json', label: m.route_tab_json() },
  ])
  const ruleItems = $derived(routes.map((rule, index) => {
    const record = asRecord(rule) ?? {}
    const matchCount = matchesOf(record).length
    return {
      id: String(index),
      title: `${m.route_rule_index({ index: String(index + 1) })} · ${matchCount === 0 ? m.route_match_all_tag() : m.route_match_count({ count: String(matchCount) })}`,
    }
  }))

  $effect(() => {
    if (store.reloadEpoch !== seenEpoch) {
      seenEpoch = store.reloadEpoch
      if (open)
        open = false
    }
  })

  $effect(() => {
    if (!open) {
      initializedFor = undefined
      return
    }
    const target = routeName
    if (initializedFor === target)
      return
    initializedFor = target
    void initialize(target)
  })

  async function initialize(name: string | null) {
    deleting = false
    store.error = ''
    if (name) {
      // 每次打开都重新读取，确保编辑器持有本次快照对应的版本。
      draft = {}
      await store.loadRoute(name)
      draft = cloneRecord(store.route)
      originalName = name
      createType = routeTypeOf(draft)
    }
    else {
      originalName = null
      createType = 'http'
      draft = newRoute('new-route', createType)
    }
    // 插件属性与实例需要同时就绪，否则插件选择器只剩空白页签。
    if (store.plugins.length === 0 || store.pluginAttrs.length === 0)
      await store.loadPlugins()
  }

  /** 打开插件实例编辑器；创建成功后立即回调，由调用方决定加入哪一条插件链。 */
  function openPluginCreate(code: string, onCreated?: (instance: PluginInstance) => void) {
    pendingPluginBinding = onCreated ?? null
    pluginCreateCode = code
    store.selectedPluginCode = code
    pluginEditorOpen = true
  }

  function openPicker() {
    pickerOpen = true
  }

  /** 选择器选中已有实例时追加到路由级插件链；选择“新建实例”时转入编辑器。 */
  function handlePick(pick: PluginPick) {
    if (pick.type === 'instance' && pick.instance)
      setField('plugins', [...routePlugins, createBinding(pick.instance.ref, 0)])
    else if (pick.type === 'plugin' && pick.code)
      openPluginCreate(pick.code, (instance) => { setField('plugins', [...routePlugins, createBinding(instance.ref, 0)]) })
  }

  function handlePluginCreated(instance: PluginInstance) {
    pendingPluginBinding?.(instance)
    pendingPluginBinding = null
  }

  function setField(key: string, value: unknown) {
    draft = { ...cloneRecord(draft), [key]: value }
  }

  function switchType(type: RouteType) {
    if (!isCreate)
      return
    createType = type
    const name = readString(draft, 'route_name') || 'new-route'
    draft = newRoute(name, type)
  }

  function ruleRecord(index: number): JsonRecord {
    return asRecord(routes[index]) ?? {}
  }

  function setRule(index: number, patch: Record<string, unknown>) {
    const items = routes.map(item => cloneRecord(item))
    items[index] = { ...items[index], ...patch }
    setField('rules', items)
  }

  function addRule() {
    setField('rules', [...routes, newHttpRule()])
    expandedRules = [...expandedRules, String(routes.length)]
  }

  function removeRule(index: number) {
    if (routes.length <= 1) {
      store.error = m.route_rule_min_hint()
      return
    }
    setField('rules', routes.filter((_, itemIndex) => itemIndex !== index))
    expandedRules = []
  }

  /** 规则级“匹配全部”：关掉所有匹配条件时写 null，打开时补一条默认路径匹配。 */
  function setMatchAll(ruleIndex: number, checked: boolean) {
    setRule(ruleIndex, { matches: checked ? null : [{ path: { kind: 'Prefix', value: '/', replace: null }, header: null, query: null, method: null }] })
  }

  /** 快速示例：与旧管理端一致的三种 HTTP 模板，保留已填写的路由名。 */
  function applyTemplate(type: 'api' | 'static' | 'k8s') {
    if (mcp)
      return
    const current = readString(draft, 'route_name')
    const routeName = current && current !== 'new-route' ? current : `${type}-route`
    const template: JsonRecord = type === 'api'
      ? { route_name: routeName, hostnames: null, priority: 100, plugins: [], rules: [{ matches: [{ path: { kind: 'Prefix', value: '/api/', replace: '/' }, header: null, query: null, method: null }], plugins: [], backends: [{ ...newBackend(), host: { kind: 'Host', host: '127.0.0.1' }, port: 8080 }], timeout_ms: 5000, timeout_mode: null, balance_policy: null }] }
      : type === 'static'
        ? { route_name: routeName, hostnames: null, priority: 0, plugins: [], rules: [{ matches: [{ path: { kind: 'Prefix', value: '/', replace: null }, header: null, query: null, method: null }], plugins: [], backends: [{ ...newBackend(), host: { kind: 'File', path: '/var/www/spacegate-admin' }, port: null }], timeout_ms: null, timeout_mode: null, balance_policy: null }] }
        : { route_name: routeName, hostnames: ['api.example.com'], priority: 50, plugins: [], rules: [{ matches: [{ path: { kind: 'Prefix', value: '/', replace: null }, header: null, query: null, method: null }], plugins: [], backends: [{ ...newBackend(), host: { kind: 'K8sService', name: 'service-name', namespace: 'default' }, port: 80 }], timeout_ms: 5000, timeout_mode: null, balance_policy: null }] }
    draft = template
    expandedRules = ['0']
  }

  function matchesOf(rule: JsonRecord): unknown[] {
    return Array.isArray(rule.matches) ? rule.matches : []
  }

  function setMatch(ruleIndex: number, matchIndex: number, patch: Record<string, unknown>) {
    const terms = matchesOf(ruleRecord(ruleIndex)).map(item => cloneRecord(item))
    terms[matchIndex] = { ...terms[matchIndex], ...patch }
    setRule(ruleIndex, { matches: terms })
  }

  function setPath(ruleIndex: number, matchIndex: number, patch: Record<string, unknown>) {
    const term = asRecord(matchesOf(ruleRecord(ruleIndex))[matchIndex]) ?? {}
    setMatch(ruleIndex, matchIndex, { path: { ...cloneRecord(readRecord(term, 'path')), ...patch } })
  }

  function addMatch(ruleIndex: number) {
    const terms = matchesOf(ruleRecord(ruleIndex)).map(item => cloneRecord(item))
    terms.push({ path: { kind: 'Prefix', value: '/', replace: null }, header: null, query: null, method: null })
    setRule(ruleIndex, { matches: terms })
  }

  function removeMatch(ruleIndex: number, matchIndex: number) {
    const terms = matchesOf(ruleRecord(ruleIndex)).filter((_, index) => index !== matchIndex)
    setRule(ruleIndex, { matches: terms.length > 0 ? terms : null })
  }

  function toggleMethod(ruleIndex: number, matchIndex: number, method: string, checked: boolean) {
    const term = asRecord(matchesOf(ruleRecord(ruleIndex))[matchIndex]) ?? {}
    const current = Array.isArray(term.method) ? term.method.filter((item): item is string => typeof item === 'string') : []
    const next = checked ? [...new Set([...current, method])] : current.filter(item => item !== method)
    setMatch(ruleIndex, matchIndex, { method: next.length > 0 ? next : null })
  }

  function headerList(term: JsonRecord): unknown[] {
    return Array.isArray(term.header) ? term.header : []
  }

  function setHeader(ruleIndex: number, matchIndex: number, headerIndex: number, patch: Record<string, unknown>) {
    const term = asRecord(matchesOf(ruleRecord(ruleIndex))[matchIndex]) ?? {}
    const headers = headerList(term).map(item => cloneRecord(item))
    headers[headerIndex] = { ...headers[headerIndex], ...patch }
    setMatch(ruleIndex, matchIndex, { header: headers })
  }

  function addHeader(ruleIndex: number, matchIndex: number) {
    const term = asRecord(matchesOf(ruleRecord(ruleIndex))[matchIndex]) ?? {}
    setMatch(ruleIndex, matchIndex, { header: [...headerList(term), { kind: 'exact', name: '', value: '', replace: null }] })
  }

  function removeHeader(ruleIndex: number, matchIndex: number, headerIndex: number) {
    const term = asRecord(matchesOf(ruleRecord(ruleIndex))[matchIndex]) ?? {}
    const headers = headerList(term).filter((_, index) => index !== headerIndex)
    setMatch(ruleIndex, matchIndex, { header: headers.length > 0 ? headers : null })
  }

  function queryList(term: JsonRecord): unknown[] {
    return Array.isArray(term.query) ? term.query : []
  }

  function setQuery(ruleIndex: number, matchIndex: number, queryIndex: number, patch: { kind?: string, key?: string, value?: string, re?: string }) {
    const term = asRecord(matchesOf(ruleRecord(ruleIndex))[matchIndex]) ?? {}
    const queries = queryList(term).map(item => cloneRecord(item))
    const current = queries[queryIndex]
    const kind = patch.kind || readString(current, 'kind') || 'exact'
    const value = readRecord(current, 'value') ?? {}
    const nextValue = {
      key: patch.key ?? readString(value, 'key'),
      ...(kind === 'regular' ? { re: patch.re ?? readString(value, 're') } : { value: patch.value ?? readString(value, 'value') }),
    }
    queries[queryIndex] = { ...current, kind, value: nextValue }
    setMatch(ruleIndex, matchIndex, { query: queries })
  }

  function addQuery(ruleIndex: number, matchIndex: number) {
    const term = asRecord(matchesOf(ruleRecord(ruleIndex))[matchIndex]) ?? {}
    setMatch(ruleIndex, matchIndex, { query: [...queryList(term), { kind: 'exact', value: { key: '', value: '' } }] })
  }

  function removeQuery(ruleIndex: number, matchIndex: number, queryIndex: number) {
    const term = asRecord(matchesOf(ruleRecord(ruleIndex))[matchIndex]) ?? {}
    const queries = queryList(term).filter((_, index) => index !== queryIndex)
    setMatch(ruleIndex, matchIndex, { query: queries.length > 0 ? queries : null })
  }

  function backendList(ruleIndex: number | null): unknown[] {
    return ruleIndex === null ? mcpBackends : Array.isArray(ruleRecord(ruleIndex).backends) ? ruleRecord(ruleIndex).backends as unknown[] : []
  }

  function setBackend(ruleIndex: number | null, backendIndex: number, patch: Record<string, unknown>) {
    const items = backendList(ruleIndex).map(item => cloneRecord(item))
    items[backendIndex] = { ...items[backendIndex], ...patch }
    if (ruleIndex === null)
      setField('backends', items)
    else
      setRule(ruleIndex, { backends: items })
  }

  function setBackendHost(ruleIndex: number | null, backendIndex: number, kind: string) {
    const backend = asRecord(backendList(ruleIndex)[backendIndex]) ?? {}
    if (kind === 'K8sService')
      setBackend(ruleIndex, backendIndex, { host: { kind, name: 'service-name', namespace: 'default' } })
    else if (kind === 'File')
      setBackend(ruleIndex, backendIndex, { host: { kind, path: '/var/www/html' }, port: null })
    else
      setBackend(ruleIndex, backendIndex, { host: { kind: 'Host', host: readString(readRecord(backend, 'host'), 'host') || '127.0.0.1' } })
  }

  function addBackend(ruleIndex: number | null) {
    const items = [...backendList(ruleIndex), newBackend()]
    if (ruleIndex === null)
      setField('backends', items)
    else
      setRule(ruleIndex, { backends: items })
  }

  function removeBackend(ruleIndex: number | null, backendIndex: number) {
    const items = backendList(ruleIndex).filter((_, index) => index !== backendIndex)
    if (ruleIndex === null)
      setField('backends', items)
    else
      setRule(ruleIndex, { backends: items })
  }

  function setTransport(value: string) {
    setField('transport', value)
    if (value === 'legacy_sse') {
      if (!readRecord(draft, 'legacy_sse'))
        setField('legacy_sse', { sse_path: '/sse', message_path: '/message' })
    }
    else {
      setField('legacy_sse', null)
    }
  }

  function setRouteJson(value: unknown) {
    const next = asRecord(value)
    if (next)
      draft = cloneRecord(next)
  }

  function validate(): boolean {
    const name = readString(draft, 'route_name').trim()
    if (!name) {
      store.error = m.spacegate_name_required()
      return false
    }
    if (mcp) {
      const path = transport === 'legacy_sse' ? readRecord(draft, 'legacy_sse') : draft
      const paths = transport === 'legacy_sse' ? [readString(path, 'sse_path'), readString(path, 'message_path')] : [readString(path, 'path')]
      if (paths.some(item => !item.startsWith('/'))) {
        store.error = m.validation_path()
        return false
      }
      if (mcpBackends.length === 0) {
        store.error = m.validation_backend()
        return false
      }
    }
    else if (routes.length === 0) {
      store.error = m.validation_rule()
      return false
    }
    else {
      for (const rule of routes) {
        for (const match of matchesOf(ruleRecord(routes.indexOf(rule)))) {
          const path = readRecord(asRecord(match), 'path')
          const value = readString(path, 'value')
          const replace = readString(path, 'replace')
          if ((value && !value.startsWith('/')) || (replace && !replace.startsWith('/'))) {
            store.error = m.validation_path()
            return false
          }
        }
      }
    }
    for (const backend of [...(mcp ? mcpBackends : []), ...routes.flatMap(rule => backendList(routes.indexOf(rule)))]) {
      const record = asRecord(backend)
      if (!record)
        continue
      const host = readRecord(record, 'host')
      const port = readNumber(record, 'port')
      const weight = readNumber(record, 'weight')
      if (readString(host, 'kind') !== 'File' && (port === undefined || port < 1 || port > 65535)) {
        store.error = m.validation_port()
        return false
      }
      if (weight !== undefined && weight < 0) {
        store.error = m.validation_weight()
        return false
      }
    }
    return true
  }

  async function save() {
    if (!validate())
      return
    const name = readString(draft, 'route_name').trim()
    let saved = false
    if (!originalName)
      saved = await store.saveRoute(true, name, draft)
    else if (originalName !== name)
      saved = await store.renameRoute(originalName, name, draft)
    else
      saved = await store.saveRoute(false, originalName, draft)
    if (saved)
      open = false
  }

  async function confirmDelete() {
    if (originalName && await store.deleteRoute(originalName)) {
      deleting = false
      open = false
    }
  }
</script>

<Drawer bind:open title={isCreate ? m.route_create() : m.route_edit()} position="right" size="4xl" closeOnBackdrop={!store.saving}>
  <div class="flex h-[calc(100vh-7rem)] min-h-0 flex-col gap-3" data-testid="route-editor">
    <div class="min-h-0 flex-1 space-y-3 overflow-y-auto pr-1">
      {#if store.conflict}<VersionConflictBanner {store} />{/if}

      <Alert variant="info">{m.route_reload_hint()}</Alert>

      {#if !mcp}
        <div class="flex flex-wrap items-center gap-2 rounded-xl bg-base-200/50 px-3 py-2">
          <span class="text-xs font-medium text-base-content/60">{m.route_examples()}</span>
          <Button size="xs" variant="outline" onclick={() => applyTemplate('api')}><span class="icon-[tabler--book] size-3.5" aria-hidden="true"></span>{m.route_example_api()}</Button>
          <Button size="xs" variant="outline" onclick={() => applyTemplate('static')}><span class="icon-[tabler--book] size-3.5" aria-hidden="true"></span>{m.route_example_static()}</Button>
          <Button size="xs" variant="outline" onclick={() => applyTemplate('k8s')}><span class="icon-[tabler--book] size-3.5" aria-hidden="true"></span>{m.route_example_k8s()}</Button>
        </div>
      {/if}

      <Tabs items={tabItems} active={activeTab} onchange={(key) => { activeTab = key }} />

      {#if activeTab === 'basic'}
        <section class="space-y-3">
          <div>
            <h3 class="text-sm font-semibold">{m.route_basic_title()}</h3>
            <p class="mt-0.5 text-xs text-base-content/50">{mcp ? m.route_mcp_basic_desc() : m.route_basic_desc()}</p>
          </div>
          <FormLayout columns={2}>
            {#if isCreate}
              <FormField label={m.route_type()}>
                <Select value={routeType} options={[{ value: 'http', label: m.route_type_http() }, { value: 'mcp', label: m.route_type_mcp() }]} onchange={value => switchType(value as RouteType)} />
              </FormField>
            {/if}
            <FormField label={m.route_name()} required>
              <Input value={readString(draft, 'route_name')} required maxlength={128} oninput={event => setField('route_name', event.currentTarget.value)} data-testid="route-name" />
            </FormField>
            {#if !mcp}
              <FormField label={m.route_priority()}>
                <Input type="number" step={1} value={String(readNumber(draft, 'priority') ?? 0)} oninput={event => setField('priority', Number(event.currentTarget.value) || 0)} />
              </FormField>
            {/if}
            <FormField label={m.route_hostnames()}>
              <TagInput tags={hostnames} placeholder={m.route_hostnames_placeholder()} onchange={(tags: string[]) => setField('hostnames', tags.length > 0 ? tags : null)} />
            </FormField>
          </FormLayout>
        </section>
      {:else if activeTab === 'transport' && mcp}
        <section class="space-y-3">
          <h3 class="text-sm font-semibold">{m.route_transport()}</h3>
          <FormLayout columns={2}>
            <FormField label={m.route_transport()}>
              <Select value={transport} options={transportOptions} onchange={setTransport} />
            </FormField>
            {#if transport === 'legacy_sse'}
              <FormField label={m.route_sse_path()} required>
                <Input value={readString(readRecord(draft, 'legacy_sse'), 'sse_path')} required oninput={event => setField('legacy_sse', { ...cloneRecord(readRecord(draft, 'legacy_sse')), sse_path: event.currentTarget.value })} />
              </FormField>
              <FormField label={m.route_message_path()} required>
                <Input value={readString(readRecord(draft, 'legacy_sse'), 'message_path')} required oninput={event => setField('legacy_sse', { ...cloneRecord(readRecord(draft, 'legacy_sse')), message_path: event.currentTarget.value })} />
              </FormField>
            {:else}
              <FormField label={m.route_transport_path()} required>
                <Input value={readString(draft, 'path')} required oninput={event => setField('path', event.currentTarget.value)} />
              </FormField>
            {/if}
            <FormField label={m.route_timeout_mode()}>
              <Select value={readString(draft, 'timeout_mode')} options={timeoutModeOptions} onchange={value => setField('timeout_mode', value || 'disabled')} />
            </FormField>
            <FormField label={m.route_session_affinity()}>
              <Select value={readString(draft, 'session_affinity') || 'mcp_session'} options={[{ value: 'mcp_session', label: 'mcp_session' }, { value: 'none', label: 'none' }]} onchange={value => setField('session_affinity', value)} />
            </FormField>
          </FormLayout>
        </section>
      {:else if activeTab === 'match'}
        <section class="space-y-3">
          <div>
            <h3 class="text-sm font-semibold">{m.route_match_title()}</h3>
            <p class="mt-0.5 text-xs text-base-content/50">{m.route_match_desc()}</p>
          </div>
          <Accordion items={ruleItems} bind:value={expandedRules} multiple variant="bordered">
            {#snippet children(item)}
              {@const ruleIndex = Number(item.id)}
              {@const record = ruleRecord(ruleIndex)}
              <div class="space-y-3">
                <div class="flex flex-wrap items-center justify-between gap-2">
                  <label class="flex items-center gap-2 text-xs">{m.route_match_all()}<Switch checked={matchesOf(record).length === 0} onchange={checked => setMatchAll(ruleIndex, checked)} /></label>
                  <Button size="xs" variant="ghost" class="text-error" onclick={() => removeRule(ruleIndex)}>{m.remove()}</Button>
                </div>
                {#if matchesOf(record).length > 0}
                  {#each matchesOf(record) as term, matchIndex (matchIndex)}
                    {@const match = asRecord(term) ?? {}}
                    {@const path = readRecord(match, 'path') ?? {}}
                    <div class="space-y-3 rounded-lg border border-base-content/10 bg-base-100 p-3">
                      <div class="flex items-center justify-between gap-2">
                        <span class="text-xs text-base-content/50">{m.route_match_index({ index: String(matchIndex + 1) })}</span>
                        <Button size="xs" variant="ghost" class="text-error" onclick={() => removeMatch(ruleIndex, matchIndex)}>{m.remove()}</Button>
                      </div>
                      <div class="grid gap-3 md:grid-cols-[7rem_minmax(0,1fr)_minmax(0,1fr)]">
                        <FormField label={m.route_path_kind()}>
                          <Select value={readString(path, 'kind') || 'Prefix'} options={pathKindOptions} onchange={kind => setPath(ruleIndex, matchIndex, { kind })} />
                        </FormField>
                        <FormField label={m.route_path_value()} required>
                          <Input value={readString(path, 'value')} placeholder="/api/" required oninput={event => setPath(ruleIndex, matchIndex, { value: event.currentTarget.value })} />
                        </FormField>
                        <FormField label={m.route_path_replace()}>
                          <Input value={readString(path, 'replace')} placeholder="/" oninput={event => setPath(ruleIndex, matchIndex, { replace: event.currentTarget.value || null })} />
                        </FormField>
                      </div>
                      <FormField label={m.route_method()}>
                        <div class="flex flex-wrap gap-3">
                          {#each methodOptions as method (method)}
                            <Checkbox label={method} checked={Array.isArray(match.method) && match.method.includes(method)} onchange={checked => toggleMethod(ruleIndex, matchIndex, method, checked)} />
                          {/each}
                        </div>
                      </FormField>

                      <div class="grid gap-3 xl:grid-cols-2">
                        <div class="space-y-2 rounded-lg bg-base-200/40 p-3">
                          <div class="flex items-start justify-between gap-2">
                            <span class="text-xs font-medium">{m.route_headers()}</span>
                            <Button size="xs" variant="ghost" onclick={() => addHeader(ruleIndex, matchIndex)}>{m.route_add_header()}</Button>
                          </div>
                          {#if headerList(match).length === 0}
                            <p class="py-2 text-center text-xs text-base-content/40">{m.no_data()}</p>
                          {:else}
                            {#each headerList(match) as header, headerIndex (headerIndex)}
                              {@const entry = asRecord(header) ?? {}}
                              <div class="space-y-1.5">
                                <div class="flex flex-wrap items-center gap-1.5">
                                  <Select class="w-24" value={readString(entry, 'kind') || 'exact'} options={headerKindOptions} onchange={kind => setHeader(ruleIndex, matchIndex, headerIndex, { kind })} />
                                  <Input class="w-28" value={readString(entry, 'name')} placeholder={m.route_header_name()} oninput={event => setHeader(ruleIndex, matchIndex, headerIndex, { name: event.currentTarget.value })} />
                                  {#if readString(entry, 'kind') === 'reg_exp'}
                                    <Input class="w-24" value={readString(entry, 're')} placeholder={m.route_header_re()} oninput={event => setHeader(ruleIndex, matchIndex, headerIndex, { re: event.currentTarget.value })} />
                                  {:else}
                                    <Input class="w-24" value={readString(entry, 'value')} placeholder={m.route_header_value()} oninput={event => setHeader(ruleIndex, matchIndex, headerIndex, { value: event.currentTarget.value })} />
                                  {/if}
                                  <Button size="xs" variant="ghost" class="text-error" onclick={() => removeHeader(ruleIndex, matchIndex, headerIndex)}>{m.remove()}</Button>
                                </div>
                                <Input value={readString(entry, 'replace')} placeholder={m.route_header_replace()} oninput={event => setHeader(ruleIndex, matchIndex, headerIndex, { replace: event.currentTarget.value || null })} />
                              </div>
                            {/each}
                          {/if}
                        </div>

                        <div class="space-y-2 rounded-lg bg-base-200/40 p-3">
                          <div class="flex items-start justify-between gap-2">
                            <span class="text-xs font-medium">{m.route_queries()}</span>
                            <Button size="xs" variant="ghost" onclick={() => addQuery(ruleIndex, matchIndex)}>{m.route_add_query()}</Button>
                          </div>
                          {#if queryList(match).length === 0}
                            <p class="py-2 text-center text-xs text-base-content/40">{m.no_data()}</p>
                          {:else}
                            {#each queryList(match) as query, queryIndex (queryIndex)}
                              {@const entry = asRecord(query) ?? {}}
                              {@const value = readRecord(entry, 'value') ?? {}}
                              <div class="flex flex-wrap items-center gap-1.5">
                                <Select class="w-24" value={readString(entry, 'kind') || 'exact'} options={queryKindOptions} onchange={kind => setQuery(ruleIndex, matchIndex, queryIndex, { kind })} />
                                <Input class="w-28" value={readString(value, 'key')} placeholder={m.route_query_key()} oninput={event => setQuery(ruleIndex, matchIndex, queryIndex, { key: event.currentTarget.value })} />
                                {#if readString(entry, 'kind') === 'regular'}
                                  <Input class="w-24" value={readString(value, 're')} placeholder={m.route_query_re()} oninput={event => setQuery(ruleIndex, matchIndex, queryIndex, { re: event.currentTarget.value })} />
                                {:else}
                                  <Input class="w-24" value={readString(value, 'value')} placeholder={m.route_query_value()} oninput={event => setQuery(ruleIndex, matchIndex, queryIndex, { value: event.currentTarget.value })} />
                                {/if}
                                <Button size="xs" variant="ghost" class="text-error" onclick={() => removeQuery(ruleIndex, matchIndex, queryIndex)}>{m.remove()}</Button>
                              </div>
                            {/each}
                          {/if}
                        </div>
                      </div>
                    </div>
                  {/each}
                  <Button size="xs" variant="outline" class="w-full" onclick={() => addMatch(ruleIndex)}>{m.route_add_match()}</Button>
                {/if}
                <FormLayout columns={2}>
                  <FormField label={m.route_rule_timeout()}>
                    <Input type="number" min={0} step={100} value={String(readNumber(record, 'timeout_ms') ?? '')} oninput={event => setRule(ruleIndex, { timeout_ms: event.currentTarget.value ? Number(event.currentTarget.value) : null })} />
                  </FormField>
                  <FormField label={m.route_rule_balance_policy()}>
                    <Select value={readString(record, 'balance_policy')} options={balanceOptions} onchange={value => setRule(ruleIndex, { balance_policy: value || null })} />
                  </FormField>
                </FormLayout>
              </div>
            {/snippet}
          </Accordion>
          <Button variant="outline" class="w-full" onclick={addRule}>{m.route_add_rule()}</Button>
        </section>
      {:else if activeTab === 'backend'}
        <section class="space-y-3">
          <div>
            <h3 class="text-sm font-semibold">{m.route_backend_title()}</h3>
            <p class="mt-0.5 text-xs text-base-content/50">{mcp ? m.route_mcp_backend_desc() : m.route_backend_desc()}</p>
          </div>
          {#if mcp}
            {#each backendList(null) as backend, index (index)}
              {@render backendFields(null, backend, index)}
            {/each}
            <Button variant="outline" class="w-full" onclick={() => addBackend(null)}>{m.route_add_backend()}</Button>
          {:else}
            <Accordion items={ruleItems} bind:value={expandedRules} multiple variant="bordered">
              {#snippet children(item)}
                {@const ruleIndex = Number(item.id)}
                <div class="space-y-3">
                  {#each backendList(ruleIndex) as backend, backendIndex (backendIndex)}
                    {@render backendFields(ruleIndex, backend, backendIndex)}
                  {/each}
                  <Button variant="outline" class="w-full" onclick={() => addBackend(ruleIndex)}>{m.route_add_backend()}</Button>
                </div>
              {/snippet}
            </Accordion>
          {/if}
        </section>
      {:else if activeTab === 'plugins'}
        <section class="space-y-3">
          <div>
            <h3 class="text-sm font-semibold">{m.route_plugin_title()}</h3>
            <p class="mt-0.5 text-xs text-base-content/50">{m.route_plugin_chain_desc()}</p>
          </div>
          <PluginBindingEditor bindings={routePlugins} instances={store.plugins} disabled={store.saving} onrequestPick={openPicker} onchange={items => setField('plugins', items)} />
        </section>
      {:else if activeTab === 'json'}
        <section class="space-y-2">
          <div>
            <h3 class="text-sm font-semibold">{m.route_json_title()}</h3>
            <p class="mt-0.5 text-xs text-base-content/50">{m.route_json_desc()}</p>
          </div>
          <JsonTextarea value={draft} rows={18} disabled={store.saving} onchange={setRouteJson} testid="route-json" />
        </section>
      {/if}

      {#if store.error}<p class="text-sm text-error" role="alert">{store.error}</p>{/if}

      {#if deleting}
        <div class="rounded-xl border border-error/30 bg-error/5 p-3">
          <p class="text-sm">{m.route_delete_confirm({ name: originalName ?? '' })}</p>
          <div class="mt-3 flex justify-end gap-2">
            <Button variant="ghost" size="sm" disabled={store.saving} onclick={() => { deleting = false }}>{m.cancel()}</Button>
            <Button variant="error" size="sm" loading={store.saving} onclick={() => void confirmDelete()}>{m.delete()}</Button>
          </div>
        </div>
      {/if}
    </div>

    <div class="flex shrink-0 flex-wrap items-center justify-between gap-2 border-t border-base-content/8 pt-3">
      <div>
        {#if !isCreate && !deleting}<Button variant="ghost" size="sm" class="text-error" disabled={store.saving} onclick={() => { deleting = true }}>{m.delete()}</Button>{/if}
      </div>
      <div class="flex gap-2">
        <Button variant="ghost" disabled={store.saving} onclick={() => { open = false }}>{m.cancel()}</Button>
        <Button variant="primary" loading={store.saving} onclick={() => void save()} data-testid="route-save">{m.save()}</Button>
      </div>
    </div>
  </div>
</Drawer>

<PluginPicker bind:open={pickerOpen} {store} instances={store.plugins} onpick={handlePick} />
<PluginEditor bind:open={pluginEditorOpen} instanceKey={null} createCode={pluginCreateCode} {store} oncreated={handlePluginCreated} />

{#snippet backendFields(ruleIndex: number | null, backend: unknown, backendIndex: number)}
  {@const record = asRecord(backend) ?? {}}
  {@const host = readRecord(record, 'host') ?? {}}
  {@const kind = readString(host, 'kind') || 'Host'}
  <div class="space-y-2 rounded-lg border border-base-content/10 p-3">
    <div class="flex items-center justify-between gap-2">
      <span class="text-xs text-base-content/50">{m.route_backend_index({ index: String(backendIndex + 1) })}</span>
      <Button size="xs" variant="ghost" class="text-error" onclick={() => removeBackend(ruleIndex, backendIndex)}>{m.remove()}</Button>
    </div>
    <FormLayout columns={2}>
      <FormField label={m.route_backend_kind()}>
        <Select value={kind} options={hostKindOptions} onchange={value => setBackendHost(ruleIndex, backendIndex, value)} />
      </FormField>
      {#if kind === 'K8sService'}
        <FormField label={m.route_backend_service()}>
          <Input value={readString(host, 'name')} oninput={event => setBackend(ruleIndex, backendIndex, { host: { ...host, name: event.currentTarget.value } })} />
        </FormField>
        <FormField label={m.route_backend_namespace()}>
          <Input value={readString(host, 'namespace')} oninput={event => setBackend(ruleIndex, backendIndex, { host: { ...host, namespace: event.currentTarget.value } })} />
        </FormField>
      {:else if kind === 'File'}
        <FormField label={m.route_backend_file_path()}>
          <Input value={readString(host, 'path')} oninput={event => setBackend(ruleIndex, backendIndex, { host: { ...host, path: event.currentTarget.value } })} />
        </FormField>
      {:else}
        <FormField label={m.route_backend_host()}>
          <Input value={readString(host, 'host')} oninput={event => setBackend(ruleIndex, backendIndex, { host: { ...host, host: event.currentTarget.value } })} />
        </FormField>
      {/if}
      {#if kind !== 'File'}
        <FormField label={m.route_backend_port()}>
          <Input type="number" min={1} max={65535} step={1} value={String(readNumber(record, 'port') ?? 80)} oninput={event => setBackend(ruleIndex, backendIndex, { port: Number(event.currentTarget.value) })} />
        </FormField>
      {/if}
      <FormField label={m.route_backend_protocol()}>
        <Select value={readString(record, 'protocol')} options={protocolOptions} onchange={value => setBackend(ruleIndex, backendIndex, { protocol: value || null })} />
      </FormField>
      <FormField label={m.route_backend_weight()}>
        <Input type="number" min={0} step={1} value={String(readNumber(record, 'weight') ?? 1)} oninput={event => setBackend(ruleIndex, backendIndex, { weight: event.currentTarget.value ? Number(event.currentTarget.value) : null })} />
      </FormField>
      <FormField label={m.route_backend_timeout()}>
        <Input type="number" min={0} step={100} value={String(readNumber(record, 'timeout_ms') ?? '')} oninput={event => setBackend(ruleIndex, backendIndex, { timeout_ms: event.currentTarget.value ? Number(event.currentTarget.value) : null })} />
      </FormField>
      <FormField label={m.route_backend_timeout_mode()}>
        <Select value={readString(record, 'timeout_mode')} options={timeoutModeOptions} onchange={(value: string) => setBackend(ruleIndex, backendIndex, { timeout_mode: value || null })} />
      </FormField>
      <FormField label={m.route_backend_downgrade_http2()}>
        <Switch checked={readBoolean(record, 'downgrade_http2') === true} onchange={checked => setBackend(ruleIndex, backendIndex, { downgrade_http2: checked })} />
      </FormField>
    </FormLayout>
  </div>
{/snippet}
