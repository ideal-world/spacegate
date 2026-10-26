<script lang="ts">
  import type { ManagementStore } from '../../stores/management-store.svelte.js'
  import type { JsonRecord } from '../../spacegate/model.js'
  import { FormField, FormLayout, Modal } from '@h-ai/ui/components/compounds/index'
  import { Button, Input, Select, Switch, Textarea } from '@h-ai/ui/components/primitives/index'
  import * as m from '../../paraglide/messages.js'
  import {
    asRecord,
    cloneRecord,
    newGateway,
    newListener,
    readBoolean,
    readNumber,
    readRecord,
    readString,
  } from '../../spacegate/model.js'
  import JsonTextarea from './JsonTextarea.svelte'
  import PluginBindingEditor from './PluginBindingEditor.svelte'
  import VersionConflictBanner from './VersionConflictBanner.svelte'

  interface Props {
    store: ManagementStore
    open?: boolean
    gatewayName?: string | null
  }
  let { store, open = $bindable(false), gatewayName = null }: Props = $props()

  let draft = $state<JsonRecord>(newGateway('new-gateway'))
  let gatewayNameInput = $state('new-gateway')
  let deleting = $state(false)
  let initializedFor = $state<string | null>(null)
  let seenEpoch = $state(0)

  const isCreate = $derived(!gatewayName)
  const parameters = $derived(readRecord(draft, 'parameters') ?? {})
  const listeners = $derived(Array.isArray(draft.listeners) ? draft.listeners : [])
  const bindings = $derived(Array.isArray(draft.plugins) ? draft.plugins : [])

  const protocolOptions = [{ value: 'http', label: 'HTTP' }, { value: 'https', label: 'HTTPS' }]
  const tlsModeOptions = [{ value: 'Terminate', label: 'Terminate' }, { value: 'Passthrough', label: 'Passthrough' }]
  const logLevelOptions = [{ value: '', label: m.none() }, { value: 'trace', label: 'trace' }, { value: 'debug', label: 'debug' }, { value: 'info', label: 'info' }, { value: 'warn', label: 'warn' }, { value: 'error', label: 'error' }]
  const languageOptions = [{ value: '', label: m.none() }, { value: 'zh-CN', label: 'zh-CN' }, { value: 'en', label: 'en' }]

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
    const target = gatewayName
    if (initializedFor === target)
      return
    initializedFor = target
    void initialize(target)
  })

  async function initialize(name: string | null) {
    deleting = false
    store.error = ''
    if (name) {
      // 先清空旧草稿，避免重新打开时短暂显示或提交上一个快照；随后总是重新读取。
      draft = {}
      gatewayNameInput = name
      await store.loadGateway(name)
      draft = cloneRecord(store.gatewayConfig ?? store.gateway)
      gatewayNameInput = name
    }
    else {
      draft = newGateway('new-gateway')
      gatewayNameInput = 'new-gateway'
    }
    if (store.plugins.length === 0)
      await store.loadPlugins()
  }

  function setParameter(key: string, value: unknown) {
    draft = { ...cloneRecord(draft), parameters: { ...cloneRecord(parameters), [key]: value } }
  }

  function setListener(index: number, patch: Record<string, unknown>) {
    const items = listeners.map(item => cloneRecord(item))
    items[index] = { ...items[index], ...patch }
    draft = { ...cloneRecord(draft), listeners: items }
  }

  function setProtocol(index: number, type: string) {
    if (type === 'https')
      setListener(index, { protocol: { type: 'https', tls: { mode: 'Terminate', key: '', cert: '', http2: false } } })
    else
      setListener(index, { protocol: { type: 'http' } })
  }

  function protocolOf(listener: unknown): JsonRecord {
    return readRecord(asRecord(listener), 'protocol') ?? { type: 'http' }
  }

  function addListener() {
    draft = { ...cloneRecord(draft), listeners: [...listeners, newListener(listeners.length + 1)] }
  }

  function removeListener(index: number) {
    draft = { ...cloneRecord(draft), listeners: listeners.filter((_, itemIndex) => itemIndex !== index) }
  }

  function setObservability(value: unknown) {
    const next = asRecord(value)
    if (next)
      setParameter('observability', cloneRecord(next))
  }

  function setGatewayJson(value: unknown) {
    const next = asRecord(value)
    if (!next)
      return
    draft = cloneRecord(next)
    if (isCreate && !gatewayNameInput.trim())
      gatewayNameInput = readString(draft, 'name')
  }

  async function save() {
    const saved = await store.saveGateway(isCreate, (gatewayNameInput || readString(draft, 'name')).trim(), draft)
    if (saved)
      open = false
  }

  async function confirmDelete() {
    if (gatewayName && await store.deleteGateway(gatewayName)) {
      deleting = false
      open = false
    }
  }
</script>

<Modal bind:open title={isCreate ? m.gateway_create() : m.gateway_edit()} size="3xl" closeOnBackdrop={!store.saving} closeOnEscape={!store.saving} data-testid="gateway-editor">
  <div class="space-y-6">
    {#if store.conflict}<VersionConflictBanner {store} />{/if}

    <FormLayout columns={2}>
      <FormField label={m.gateway_name()} required>
        <Input bind:value={gatewayNameInput} readonly={!isCreate} required maxlength={128} data-testid="gateway-name" />
      </FormField>
      <FormField label={m.gateway_listeners()}>
        <div class="flex items-center gap-2">
          <span class="text-sm text-base-content/60">{m.gateway_listener_count({ count: String(listeners.length) })}</span>
          <Button size="xs" variant="outline" onclick={addListener} disabled={store.saving}>{m.add()}</Button>
        </div>
      </FormField>
    </FormLayout>

    <section class="space-y-3">
      <h3 class="text-sm font-semibold">{m.gateway_listeners()}</h3>
      {#each listeners as listener, index (index)}
        {@const protocol = protocolOf(listener)}
        {@const tls = readRecord(protocol, 'tls')}
        <div class="space-y-3 rounded-xl border border-base-content/10 p-3">
          <div class="flex items-center justify-between gap-2">
            <span class="text-xs text-base-content/50">{m.gateway_listener_index({ index: String(index + 1) })}</span>
            <Button size="xs" variant="ghost" class="text-error" disabled={store.saving} onclick={() => removeListener(index)}>{m.remove()}</Button>
          </div>
          <FormLayout columns={2}>
            <FormField label={m.gateway_listener_name()} required>
              <Input value={readString(asRecord(listener), 'name')} required oninput={event => setListener(index, { name: event.currentTarget.value })} />
            </FormField>
            <FormField label={m.gateway_listener_protocol()}>
              <Select value={readString(protocol, 'type') || 'http'} options={protocolOptions} onchange={type => setProtocol(index, type)} />
            </FormField>
            <FormField label={m.gateway_listener_ip()}>
              <Input value={readString(asRecord(listener), 'ip')} placeholder="0.0.0.0" oninput={event => setListener(index, { ip: event.currentTarget.value || null })} />
            </FormField>
            <FormField label={m.gateway_listener_port()} required>
              <Input type="number" min={0} max={65535} step={1} value={String(readNumber(asRecord(listener), 'port') ?? 8080)} oninput={event => setListener(index, { port: Number(event.currentTarget.value) })} />
            </FormField>
            <FormField label={m.gateway_listener_hostname()}>
              <Input value={readString(asRecord(listener), 'hostname')} oninput={event => setListener(index, { hostname: event.currentTarget.value || null })} />
            </FormField>
          </FormLayout>
          {#if readString(protocol, 'type') === 'https'}
            <div class="space-y-3 rounded-lg bg-base-200/40 p-3">
              <div class="flex flex-wrap items-center justify-between gap-2">
                <span class="text-xs font-medium">{m.gateway_tls()}</span>
                <label class="flex items-center gap-2 text-xs">{m.gateway_tls_http2()}<Switch checked={readBoolean(tls, 'http2') === true} onchange={checked => setListener(index, { protocol: { ...protocol, tls: { ...cloneRecord(tls), http2: checked } } })} /></label>
              </div>
              <FormField label={m.gateway_tls_mode()}>
                <Select value={readString(tls, 'mode') || 'Terminate'} options={tlsModeOptions} onchange={mode => setListener(index, { protocol: { ...protocol, tls: { ...cloneRecord(tls), mode } } })} />
              </FormField>
              <FormField label={m.gateway_tls_cert()}>
                <Textarea rows={3} class="font-mono text-xs" value={readString(tls, 'cert')} oninput={event => setListener(index, { protocol: { ...protocol, tls: { ...cloneRecord(tls), cert: event.currentTarget.value } } })} />
              </FormField>
              <FormField label={m.gateway_tls_key()}>
                <Textarea rows={3} class="font-mono text-xs" value={readString(tls, 'key')} oninput={event => setListener(index, { protocol: { ...protocol, tls: { ...cloneRecord(tls), key: event.currentTarget.value } } })} />
              </FormField>
            </div>
          {/if}
        </div>
      {/each}
      {#if listeners.length === 0}<p class="text-xs text-base-content/45">{m.gateway_no_listener()}</p>{/if}
    </section>

    <section class="space-y-3">
      <h3 class="text-sm font-semibold">{m.gateway_parameters()}</h3>
      <FormLayout columns={2}>
        <FormField label={m.gateway_redis_url()}>
          <Input value={readString(parameters, 'redis_url')} placeholder="redis://127.0.0.1:6379" oninput={event => setParameter('redis_url', event.currentTarget.value || null)} />
        </FormField>
        <FormField label={m.gateway_redis_pool_max_size()}>
          <Input type="number" min={1} step={1} value={String(readNumber(parameters, 'redis_pool_max_size') ?? '')} oninput={event => setParameter('redis_pool_max_size', event.currentTarget.value ? Number(event.currentTarget.value) : null)} />
        </FormField>
        <FormField label={m.gateway_redis_wait_timeout()}>
          <Input type="number" min={1} step={100} value={String(readNumber(parameters, 'redis_pool_wait_timeout_ms') ?? '')} oninput={event => setParameter('redis_pool_wait_timeout_ms', event.currentTarget.value ? Number(event.currentTarget.value) : null)} />
        </FormField>
        <FormField label={m.gateway_redis_create_timeout()}>
          <Input type="number" min={1} step={100} value={String(readNumber(parameters, 'redis_pool_create_timeout_ms') ?? '')} oninput={event => setParameter('redis_pool_create_timeout_ms', event.currentTarget.value ? Number(event.currentTarget.value) : null)} />
        </FormField>
        <FormField label={m.gateway_redis_recycle_timeout()}>
          <Input type="number" min={1} step={100} value={String(readNumber(parameters, 'redis_pool_recycle_timeout_ms') ?? '')} oninput={event => setParameter('redis_pool_recycle_timeout_ms', event.currentTarget.value ? Number(event.currentTarget.value) : null)} />
        </FormField>
        <FormField label={m.gateway_log_level()}>
          <Select value={readString(parameters, 'log_level')} options={logLevelOptions} onchange={value => setParameter('log_level', value || null)} />
        </FormField>
        <FormField label={m.gateway_language()}>
          <Select value={readString(parameters, 'lang')} options={languageOptions} onchange={value => setParameter('lang', value || null)} />
        </FormField>
        <FormField label={m.gateway_x_request_id()}>
          <Switch checked={readBoolean(parameters, 'enable_x_request_id') === true} onchange={checked => setParameter('enable_x_request_id', checked)} />
        </FormField>
        <FormField label={m.gateway_ignore_tls()}>
          <Switch checked={readBoolean(parameters, 'ignore_tls_verification') === true} onchange={checked => setParameter('ignore_tls_verification', checked)} />
        </FormField>
      </FormLayout>
      <FormField label={m.gateway_observability()}>
        <JsonTextarea value={readRecord(parameters, 'observability') ?? {}} rows={8} onchange={setObservability} />
      </FormField>
    </section>

    <section class="space-y-3">
      <h3 class="text-sm font-semibold">{m.gateway_plugins()}</h3>
      <PluginBindingEditor bindings={bindings} instances={store.plugins} disabled={store.saving} onchange={(items) => { draft = { ...cloneRecord(draft), plugins: items } }} />
    </section>

    <section class="space-y-2">
      <h3 class="text-sm font-semibold">{m.gateway_advanced()}</h3>
      <p class="text-xs text-base-content/50">{m.gateway_json_hint()}</p>
      <JsonTextarea value={draft} rows={14} disabled={store.saving} onchange={setGatewayJson} testid="gateway-json" />
    </section>

    {#if store.error}<p class="text-sm text-error" role="alert">{store.error}</p>{/if}

    {#if deleting}
      <div class="rounded-xl border border-error/30 bg-error/5 p-3">
        <p class="text-sm">{m.gateway_delete_confirm({ name: gatewayName ?? '' })}</p>
        <p class="mt-1 text-xs text-base-content/55">{m.gateway_delete_hint()}</p>
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
        <Button variant="primary" loading={store.saving} onclick={() => void save()} data-testid="gateway-save">{m.save()}</Button>
      </div>
    </div>
  </div>
</Modal>
