<script lang="ts">
  import type { ManagementStore } from '../../stores/management-store.svelte.js'
  import type { JsonRecord, RouteType } from '../../spacegate/model.js'
  import { FormField, FormLayout, Modal, TagInput } from '@h-ai/ui/components/compounds/index'
  import { Button, Checkbox, Input, Select, Switch } from '@h-ai/ui/components/primitives/index'
  import * as m from '../../paraglide/messages.js'
  import {
    asRecord,
    cloneRecord,
    isMcpRoute,
    newBackend,
    newHttpRule,
    newRoute,
    readBoolean,
    readNumber,
    readRecord,
    readString,
    routeNameOf,
    routeTypeOf,
  } from '../../spacegate/model.js'
  import JsonTextarea from './JsonTextarea.svelte'
  import PluginBindingEditor from './PluginBindingEditor.svelte'
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
  let initializedFor = $state<string | null>(null)
  let seenEpoch = $state(0)

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

  $effect(() => {
    if (store.reloadEpoch !== seenEpoch) {
      seenEpoch = store.reloadEpoch
      if (open)
        open = false
    }
  })

  $effect(() => {
    if (!open) {
      initializedFor = null
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
      if (!store.route || routeNameOf(store.route) !== name)
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
    if (store.plugins.length === 0)
      await store.loadPlugins()
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
  }

  function removeRule(index: number) {
    setField('rules', routes.filter((_, itemIndex) => itemIndex !== index))
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

<Modal bind:open title={isCreate ? m.route_create() : m.route_edit()} size="3xl" closeOnBackdrop={!store.saving} closeOnEscape={!store.saving} data-testid="route-editor">
  <div class="space-y-6">
    {#if store.conflict}<VersionConflictBanner {store} />{/if}

    <FormLayout columns={2}>
      <FormField label={m.route_type()}>
        <Select value={routeType} options={[{ value: 'http', label: m.route_type_http() }, { value: 'mcp', label: m.route_type_mcp() }]} disabled={!isCreate} onchange={value => switchType(value as RouteType)} />
      </FormField>
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

    {#if mcp}
      <section class="space-y-3 border-t border-base-content/8 pt-4">
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
        <div class="flex items-center justify-between gap-2">
          <h3 class="text-sm font-semibold">{m.route_backends()}</h3>
          <Button size="xs" variant="outline" onclick={() => addBackend(null)}>{m.route_add_backend()}</Button>
        </div>
        {#each backendList(null) as backend, index (index)}
          {@render backendFields(null, backend, index)}
        {/each}
      </section>
    {:else}
      <section class="space-y-3 border-t border-base-content/8 pt-4">
        <div class="flex items-center justify-between gap-2">
          <h3 class="text-sm font-semibold">{m.route_rules()}</h3>
          <Button size="xs" variant="outline" onclick={addRule}>{m.route_add_rule()}</Button>
        </div>
        {#each routes as _rule, ruleIndex (ruleIndex)}
          {@const record = ruleRecord(ruleIndex)}
          <div class="space-y-3 rounded-xl border border-base-content/10 p-3">
            <div class="flex flex-wrap items-center justify-between gap-2">
              <span class="text-xs text-base-content/50">{m.route_rule_index({ index: String(ruleIndex + 1) })}</span>
              <div class="flex items-center gap-2">
                <label class="flex items-center gap-1 text-xs">{m.route_match_all()}<Switch checked={matchesOf(record).length === 0} onchange={checked => setRule(ruleIndex, { matches: checked ? null : [{ path: { kind: 'Prefix', value: '/', replace: null }, header: null, query: null, method: null }] })} /></label>
                <Button size="xs" variant="ghost" class="text-error" onclick={() => removeRule(ruleIndex)}>{m.remove()}</Button>
              </div>
            </div>
            {#if matchesOf(record).length > 0}
              {#each matchesOf(record) as term, matchIndex (matchIndex)}
                {@const match = asRecord(term) ?? {}}
                {@const path = readRecord(match, 'path') ?? {}}
                <div class="space-y-3 rounded-lg bg-base-200/35 p-3">
                  <div class="flex items-center justify-between gap-2">
                    <span class="text-xs text-base-content/50">{m.route_match_index({ index: String(matchIndex + 1) })}</span>
                    <Button size="xs" variant="ghost" class="text-error" onclick={() => removeMatch(ruleIndex, matchIndex)}>{m.remove()}</Button>
                  </div>
                  <FormLayout columns={2}>
                    <FormField label={m.route_path_kind()}>
                      <Select value={readString(path, 'kind') || 'Prefix'} options={pathKindOptions} onchange={kind => setPath(ruleIndex, matchIndex, { kind })} />
                    </FormField>
                    <FormField label={m.route_path_value()} required>
                      <Input value={readString(path, 'value')} required oninput={event => setPath(ruleIndex, matchIndex, { value: event.currentTarget.value })} />
                    </FormField>
                    <FormField label={m.route_path_replace()}>
                      <Input value={readString(path, 'replace')} placeholder="/" oninput={event => setPath(ruleIndex, matchIndex, { replace: event.currentTarget.value || null })} />
                    </FormField>
                  </FormLayout>
                  <FormField label={m.route_method()}>
                    <div class="flex flex-wrap gap-3">
                      {#each methodOptions as method (method)}
                        <Checkbox label={method} checked={Array.isArray(match.method) && match.method.includes(method)} onchange={checked => toggleMethod(ruleIndex, matchIndex, method, checked)} />
                      {/each}
                    </div>
                  </FormField>
                  <div class="space-y-2">
                    <div class="flex items-center justify-between gap-2">
                      <span class="text-xs font-medium">{m.route_headers()}</span>
                      <Button size="xs" variant="ghost" onclick={() => addHeader(ruleIndex, matchIndex)}>{m.route_add_header()}</Button>
                    </div>
                    {#each headerList(match) as header, headerIndex (headerIndex)}
                      {@const item = asRecord(header) ?? {}}
                      <div class="flex flex-wrap items-end gap-2">
                        <Select value={readString(item, 'kind') || 'exact'} options={headerKindOptions} onchange={kind => setHeader(ruleIndex, matchIndex, headerIndex, { kind })} />
                        <Input class="w-36" value={readString(item, 'name')} placeholder={m.route_header_name()} oninput={event => setHeader(ruleIndex, matchIndex, headerIndex, { name: event.currentTarget.value })} />
                        {#if readString(item, 'kind') === 'reg_exp'}
                          <Input class="w-36" value={readString(item, 're')} placeholder={m.route_header_re()} oninput={event => setHeader(ruleIndex, matchIndex, headerIndex, { re: event.currentTarget.value })} />
                        {:else}
                          <Input class="w-36" value={readString(item, 'value')} placeholder={m.route_header_value()} oninput={event => setHeader(ruleIndex, matchIndex, headerIndex, { value: event.currentTarget.value })} />
                        {/if}
                        <Input class="w-32" value={readString(item, 'replace')} placeholder={m.route_header_replace()} oninput={event => setHeader(ruleIndex, matchIndex, headerIndex, { replace: event.currentTarget.value || null })} />
                        <Button size="xs" variant="ghost" class="text-error" onclick={() => removeHeader(ruleIndex, matchIndex, headerIndex)}>{m.remove()}</Button>
                      </div>
                    {/each}
                  </div>
                  <div class="space-y-2">
                    <div class="flex items-center justify-between gap-2">
                      <span class="text-xs font-medium">{m.route_queries()}</span>
                      <Button size="xs" variant="ghost" onclick={() => addQuery(ruleIndex, matchIndex)}>{m.route_add_query()}</Button>
                    </div>
                    {#each queryList(match) as query, queryIndex (queryIndex)}
                      {@const item = asRecord(query) ?? {}}
                      {@const value = readRecord(item, 'value') ?? {}}
                      <div class="flex flex-wrap items-end gap-2">
                        <Select value={readString(item, 'kind') || 'exact'} options={queryKindOptions} onchange={kind => setQuery(ruleIndex, matchIndex, queryIndex, { kind })} />
                        <Input class="w-36" value={readString(value, 'key')} placeholder={m.route_query_key()} oninput={event => setQuery(ruleIndex, matchIndex, queryIndex, { key: event.currentTarget.value })} />
                        {#if readString(item, 'kind') === 'regular'}
                          <Input class="w-36" value={readString(value, 're')} placeholder={m.route_query_re()} oninput={event => setQuery(ruleIndex, matchIndex, queryIndex, { re: event.currentTarget.value })} />
                        {:else}
                          <Input class="w-36" value={readString(value, 'value')} placeholder={m.route_query_value()} oninput={event => setQuery(ruleIndex, matchIndex, queryIndex, { value: event.currentTarget.value })} />
                        {/if}
                        <Button size="xs" variant="ghost" class="text-error" onclick={() => removeQuery(ruleIndex, matchIndex, queryIndex)}>{m.remove()}</Button>
                      </div>
                    {/each}
                  </div>
                </div>
              {/each}
              <Button size="xs" variant="outline" onclick={() => addMatch(ruleIndex)}>{m.route_add_match()}</Button>
            {/if}
            <FormLayout columns={2}>
              <FormField label={m.route_rule_timeout()}>
                <Input type="number" min={0} step={100} value={String(readNumber(record, 'timeout_ms') ?? '')} oninput={event => setRule(ruleIndex, { timeout_ms: event.currentTarget.value ? Number(event.currentTarget.value) : null })} />
              </FormField>
              <FormField label={m.route_rule_balance_policy()}>
                <Select value={readString(record, 'balance_policy')} options={balanceOptions} onchange={value => setRule(ruleIndex, { balance_policy: value || null })} />
              </FormField>
            </FormLayout>
            <div class="flex items-center justify-between gap-2">
              <span class="text-xs font-medium">{m.route_backends()}</span>
              <Button size="xs" variant="ghost" onclick={() => addBackend(ruleIndex)}>{m.route_add_backend()}</Button>
            </div>
            {#each backendList(ruleIndex) as backend, backendIndex (backendIndex)}
              {@render backendFields(ruleIndex, backend, backendIndex)}
            {/each}
            <div>
              <p class="mb-2 text-xs font-medium">{m.route_rule_plugins()}</p>
              <PluginBindingEditor bindings={Array.isArray(record.plugins) ? record.plugins as unknown[] : []} instances={store.plugins} disabled={store.saving} onchange={items => setRule(ruleIndex, { plugins: items })} />
            </div>
          </div>
        {/each}
      </section>
    {/if}

    <section class="space-y-3 border-t border-base-content/8 pt-4">
      <h3 class="text-sm font-semibold">{m.route_plugins()}</h3>
      <PluginBindingEditor bindings={routePlugins} instances={store.plugins} disabled={store.saving} onchange={items => setField('plugins', items)} />
    </section>

    <section class="space-y-2 border-t border-base-content/8 pt-4">
      <h3 class="text-sm font-semibold">{m.route_json()}</h3>
      <p class="text-xs text-base-content/50">{m.route_json_hint()}</p>
      <JsonTextarea value={draft} rows={14} disabled={store.saving} onchange={setRouteJson} />
    </section>

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

    <div class="flex flex-wrap items-center justify-between gap-2 border-t border-base-content/8 pt-4">
      <div>
        {#if !isCreate && !deleting}<Button variant="ghost" size="sm" class="text-error" disabled={store.saving} onclick={() => { deleting = true }}>{m.delete()}</Button>{/if}
      </div>
      <div class="flex gap-2">
        <Button variant="ghost" disabled={store.saving} onclick={() => { open = false }}>{m.cancel()}</Button>
        <Button variant="primary" loading={store.saving} onclick={() => void save()} data-testid="route-save">{m.save()}</Button>
      </div>
    </div>
  </div>
</Modal>

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
    <div>
      <p class="mb-2 text-xs font-medium">{m.route_backend_plugins()}</p>
      <PluginBindingEditor bindings={Array.isArray(record.plugins) ? record.plugins as unknown[] : []} instances={store.plugins} disabled={store.saving} onchange={items => setBackend(ruleIndex, backendIndex, { plugins: items })} />
    </div>
  </div>
{/snippet}
