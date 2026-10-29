<script lang="ts">
  import type { ManagementStore } from '../../stores/management-store.svelte.js'
  import type { JsonRecord, PluginInstance, PluginReference } from '../../spacegate/model.js'
  import { Alert, Drawer, FormField, FormLayout, Tabs } from '@h-ai/ui/components/compounds/index'
  import { Button, Input, Select, Textarea } from '@h-ai/ui/components/primitives/index'
  import * as m from '../../paraglide/messages.js'
  import {
    asRecord,
    cloneRecord,
    formatJson,
    parseJson,
    pluginConfigOf,
    pluginIdOf,
    pluginKey,
    readString,
  } from '../../spacegate/model.js'
  import { initSchemaDefaults, schemaExample, schemaKind } from '../../spacegate/schema.js'
  import JsonTextarea from './JsonTextarea.svelte'
  import SchemaForm from './SchemaForm.svelte'
  import VersionConflictBanner from './VersionConflictBanner.svelte'

  interface Props {
    store: ManagementStore
    open?: boolean
    instanceKey?: string | null
    /** 新建时预选的插件代码；缺省时回退到 store.selectedPluginCode。 */
    createCode?: string
    /** 新建成功后回传实例引用，调用方可以立即把它加入插件链。 */
    oncreated?: (instance: PluginInstance) => void
  }
  let { store, open = $bindable(false), instanceKey = null, createCode = '', oncreated }: Props = $props()

  let original = $state<PluginInstance | null>(null)
  let code = $state('')
  let kind = $state<'named' | 'anon' | 'mono'>('named')
  let instanceName = $state('')
  let uid = $state('')
  let displayName = $state('')
  let spec = $state<JsonRecord>({})
  let editMode = $state<'form' | 'json'>('form')
  let exampleCopied = $state(false)
  let deleting = $state(false)
  let previewOpen = $state(false)
  let imageUrl = $state('')
  let schemaPath = $state('schema.json')
  let ociAuthText = $state('')
  let headers = $state<Array<{ name: string, value: string }>>([])
  let previewError = $state('')
  let initializedFor = $state<string | null>(null)
  let loadedSchemaFor = $state('')
  let seenEpoch = $state(0)

  const isCreate = $derived(original === null)
  const codeOptions = $derived(store.pluginCatalog.map(item => ({ value: item, label: item })))
  const kindOptions = [{ value: 'named', label: m.plugin_kind_named() }, { value: 'anon', label: m.plugin_kind_anon() }, { value: 'mono', label: m.plugin_kind_mono() }]
  const schema = $derived(store.pluginSchema)
  const schemaAvailable = $derived(schema !== null && schema !== undefined && schemaKind(schema, schema) !== 'json')
  /** 与旧管理端一致：无 schema 时只能编辑 JSON，有 schema 时默认表单并可切换。 */
  const activeMode = $derived(schemaAvailable ? editMode : 'json')
  const specExample = $derived(formatJson(schemaAvailable ? schemaExample(schema, schema) : {}))
  const editModeItems = $derived([{ key: 'form', label: m.plugin_mode_form() }, { key: 'json', label: m.plugin_mode_json() }])

  function applyExample() {
    const parsed = parseJson(specExample)
    if (!parsed.ok || !asRecord(parsed.value))
      return
    spec = cloneRecord(parsed.value)
    editMode = 'json'
  }

  async function copyExample() {
    if (typeof navigator === 'undefined' || !navigator.clipboard)
      return
    try {
      await navigator.clipboard.writeText(specExample)
      exampleCopied = true
    }
    catch {
      exampleCopied = false
    }
  }

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
    const target = instanceKey ?? (createCode ? `new:${createCode}` : null)
    if (initializedFor === target)
      return
    initializedFor = target
    void initialize(instanceKey)
  })

  // schema 到达后补齐默认值，但不覆盖用户已填写的字段。
  $effect(() => {
    if (!open || !code || !schema || loadedSchemaFor === code)
      return
    loadedSchemaFor = code
    const initialized = initSchemaDefaults(schema, spec, schema)
    if (asRecord(initialized))
      spec = cloneRecord(initialized)
  })

  async function initialize(key: string | null) {
    deleting = false
    previewOpen = false
    editMode = 'form'
    exampleCopied = false
    previewError = ''
    store.error = ''
    store.wasmSchema = null
    store.wasmSchemaPreview = null
    store.pluginSchema = null
    if (key) {
      let instance = store.plugins.find(item => pluginKey(item.ref) === key) ?? null
      if (!instance && store.plugin && pluginKey(store.plugin.ref) === key)
        instance = store.plugin
      if (!instance) {
        await store.loadPlugins()
        instance = store.plugins.find(item => pluginKey(item.ref) === key) ?? null
      }
      // 编辑器必须持有该实例的读取快照版本，不能复用列表/上一条实例的版本。
      // mono 插件没有可寻址的 name/uid 且不可保存，只能使用列表快照。
      if (instance && instance.ref.kind !== 'mono')
        await store.loadPlugin(instance.ref)
      const fresh = instance && instance.ref.kind !== 'mono' && store.plugin && pluginKey(store.plugin.ref) === key
        ? store.plugin
        : instance
      original = fresh
      if (fresh) {
        code = fresh.ref.code
        kind = fresh.ref.kind
        instanceName = fresh.ref.kind === 'named' ? fresh.ref.name : ''
        uid = fresh.ref.kind === 'anon' ? fresh.ref.uid : ''
        displayName = fresh.displayName
        spec = cloneRecord(fresh.spec)
        imageUrl = readString(spec, 'image_url') || readString(spec, 'url')
        schemaPath = readString(spec, 'schema_path') || 'schema.json'
        const ociAuth = asRecord(spec.oci_auth)
        ociAuthText = ociAuth ? formatJson(ociAuth) : ''
        const httpHeaders = asRecord(spec.http_headers)
        headers = httpHeaders
          ? Object.entries(httpHeaders).filter((entry): entry is [string, string] => typeof entry[1] === 'string').map(([name, value]) => ({ name, value }))
          : []
      }
    }
    else {
      original = null
      code = createCode || store.selectedPluginCode || store.pluginCatalog[0] || ''
      kind = 'named'
      instanceName = code ? `${code}-instance` : ''
      uid = ''
      displayName = ''
      spec = {}
      imageUrl = ''
      schemaPath = 'schema.json'
      ociAuthText = ''
      headers = []
    }
    loadedSchemaFor = ''
    if (code)
      await store.loadPluginSchema(code)
  }

  function kindLabel(value: string): string {
    if (value === 'named')
      return m.plugin_kind_named()
    if (value === 'anon')
      return m.plugin_kind_anon()
    return m.plugin_kind_mono()
  }

  function setCode(value: string) {
    code = value
    if (isCreate) {
      instanceName = value ? `${value}-instance` : ''
      spec = {}
      loadedSchemaFor = ''
    }
    if (value)
      void store.loadPluginSchema(value)
  }

  function currentReference(): PluginReference | null {
    if (!code)
      return null
    if (kind === 'mono')
      return { code, kind }
    if (kind === 'anon')
      return uid ? { code, kind: 'anon', uid } : null
    return instanceName ? { code, kind: 'named', name: instanceName } : null
  }

  function setSpecFromSchema(value: unknown) {
    const next = asRecord(value)
    if (next)
      spec = cloneRecord(next)
  }

  function setSpecFromJson(value: unknown) {
    const next = asRecord(value)
    if (next)
      spec = cloneRecord(next)
  }

  function setSavedSchema() {
    if (!original)
      return
    const id = pluginIdOf(original.ref)
    if (id)
      void store.loadWasmSchema(id)
  }

  function addHeader() {
    headers = [...headers, { name: '', value: '' }]
  }

  function removeHeader(index: number) {
    headers = headers.filter((_, itemIndex) => itemIndex !== index)
  }

  function setHeader(index: number, key: 'name' | 'value', value: string) {
    headers = headers.map((item, itemIndex) => itemIndex === index ? { ...item, [key]: value } : item)
  }

  async function preview() {
    previewError = ''
    if (!imageUrl.trim()) {
      previewError = m.validation_name()
      return
    }
    let ociAuth: unknown
    if (ociAuthText.trim()) {
      const parsed = parseJson(ociAuthText)
      if (!parsed.ok) {
        previewError = m.json_invalid()
        return
      }
      ociAuth = parsed.value
    }
    const httpHeaders = Object.fromEntries(headers.filter(item => item.name.trim() && item.value.trim()).map(item => [item.name.trim(), item.value]))
    await store.previewWasmSchema({
      imageUrl: imageUrl.trim(),
      schemaPath: schemaPath.trim() || undefined,
      ociAuth,
      httpHeaders: Object.keys(httpHeaders).length > 0 ? httpHeaders : undefined,
    })
  }

  async function save() {
    const reference = currentReference()
    if (!reference) {
      store.error = m.validation_name()
      return
    }
    if (kind === 'mono') {
      store.error = m.plugin_mono_readonly()
      return
    }
    const instance: PluginInstance = {
      ref: reference,
      displayName,
      spec,
      raw: original?.raw ?? {},
    }
    const config = pluginConfigOf(instance, spec, displayName)
    if (!config) {
      store.error = m.validation_name()
      return
    }
    const created = isCreate
    if (await store.savePlugin(created, config)) {
      open = false
      // 保存成功后 store 已刷新实例列表；把新实例回传给调用方，避免重新查找。
      const saved = store.plugins.find(item => pluginKey(item.ref) === pluginKey(reference)) ?? null
      if (created && saved)
        oncreated?.(saved)
    }
  }

  async function confirmDelete() {
    if (!original)
      return
    const id = pluginIdOf(original.ref)
    if (id && await store.deletePlugin(id)) {
      deleting = false
      open = false
    }
  }
</script>

<Drawer bind:open title={isCreate ? m.plugin_create() : m.plugin_edit()} position="right" size="4xl" closeOnBackdrop={!store.saving}>
  <div class="flex h-[calc(100vh-7rem)] min-h-0 flex-col gap-4" data-testid="plugin-editor">
    <div class="min-h-0 flex-1 space-y-6 overflow-y-auto pr-1">
    {#if store.conflict}<VersionConflictBanner {store} />{/if}

    <div class="flex flex-wrap items-center justify-between gap-2">
      <div class="flex flex-wrap items-center gap-2">
        <Button size="xs" variant="outline" loading={store.loading} onclick={() => void store.refreshPlugins()} data-testid="plugin-refresh">{m.plugin_refresh()}</Button>
        {#if original}
          <Button size="xs" variant="ghost" onclick={setSavedSchema}>{m.plugin_load_saved_schema()}</Button>
        {/if}
      </div>
      {#if original && pluginIdOf(original.ref) && !deleting}
        <Button size="xs" variant="ghost" class="text-error" disabled={store.saving} onclick={() => { deleting = true }}>{m.delete()}</Button>
      {/if}
    </div>

    <FormLayout columns={2}>
      <FormField label={m.plugin_display_name()}>
        <Input bind:value={displayName} maxlength={128} disabled={store.saving} />
      </FormField>
      {#if kind === 'named'}
        <FormField label={m.plugin_instance_name()} required hint={m.plugin_instance_name_hint()}>
          <Input bind:value={instanceName} required readonly={!isCreate || store.saving} />
        </FormField>
      {:else if kind === 'anon'}
        <FormField label={m.plugin_uid()} required>
          <Input bind:value={uid} required readonly />
        </FormField>
      {/if}
      {#if isCreate}
        <FormField label={m.plugin_code()} required>
          <Select value={code} options={codeOptions} placeholder={m.plugin_select_code()} disabled={store.saving} onchange={setCode} />
        </FormField>
        <FormField label={m.plugin_instance_kind()}>
          <Select value={kind} options={kindOptions} disabled={store.saving} onchange={(value) => {
 if (value === 'named' || value === 'anon' || value === 'mono')
kind = value
}} />
        </FormField>
      {:else}
        <FormField label={m.plugin_code()}><p class="font-mono text-sm">{code}</p></FormField>
        <FormField label={m.plugin_instance_kind()}><p class="text-sm">{kindLabel(kind)}</p></FormField>
      {/if}
    </FormLayout>

    {#if kind === 'mono'}
      <Alert variant="info">{m.plugin_mono_hint()}</Alert>
    {/if}

    <section class="space-y-3 border-t border-base-content/8 pt-4">
      <div class="flex flex-wrap items-start justify-between gap-2">
        <div>
          <h3 class="text-sm font-semibold">{m.plugin_runtime_spec()}</h3>
          <p class="mt-0.5 text-xs text-base-content/50">{activeMode === 'form' ? m.plugin_form_mode_desc() : m.plugin_json_mode_desc()}</p>
        </div>
        <div class="flex shrink-0 gap-1">
          <Button size="xs" variant="ghost" onclick={() => void copyExample()}>{exampleCopied ? m.json_copied() : m.plugin_copy_example()}</Button>
          <Button size="xs" variant="outline" onclick={applyExample}>{m.plugin_fill_example()}</Button>
        </div>
      </div>
      {#if schemaAvailable}
        <Tabs items={editModeItems} active={editMode} onchange={(key) => { editMode = key === 'json' ? 'json' : 'form' }} />
      {/if}
      {#if activeMode === 'json'}
        <JsonTextarea value={spec} rows={16} disabled={store.saving} onchange={setSpecFromJson} />
        {#if !schemaAvailable}<p class="text-xs text-base-content/50">{m.plugin_no_schema()}</p>{/if}
        <details class="rounded-lg border border-base-content/8">
          <summary class="cursor-pointer list-none px-3 py-2 text-xs font-medium text-base-content/60">{m.plugin_json_example()}</summary>
          <pre class="max-h-56 overflow-auto border-t border-base-content/8 p-3 font-mono text-xs">{specExample}</pre>
        </details>
      {:else}
        <SchemaForm schema={schema} value={spec} root={schema} onchange={setSpecFromSchema} />
        <p class="text-xs text-base-content/45">{m.plugin_form_hint()}</p>
      {/if}
    </section>

    <section class="space-y-3 border-t border-base-content/8 pt-4">
      <div class="flex flex-wrap items-center justify-between gap-2">
        <div>
          <h3 class="text-sm font-semibold">{m.plugin_wasm_schema()}</h3>
          <p class="mt-1 text-xs text-base-content/50">{m.plugin_wasm_hint()}</p>
        </div>
        <Button size="xs" variant="outline" onclick={() => { previewOpen = !previewOpen }}>{previewOpen ? m.close() : m.plugin_preview()}</Button>
      </div>
      {#if previewOpen}
        <div class="space-y-3 rounded-xl border border-base-content/10 p-3">
          <FormField label={m.plugin_image_url()} required>
            <Input bind:value={imageUrl} placeholder="oci://registry/repository:tag" oninput={() => { previewError = '' }} />
          </FormField>
          <FormField label={m.plugin_schema_path()}>
            <Input bind:value={schemaPath} placeholder="schema.json" />
          </FormField>
          <FormField label={m.plugin_oci_auth()}>
            <Textarea bind:value={ociAuthText} rows={4} class="font-mono text-xs" />
          </FormField>
          <div class="space-y-2">
            <div class="flex items-center justify-between gap-2">
              <span class="text-xs font-medium">{m.plugin_http_headers()}</span>
              <Button size="xs" variant="ghost" onclick={addHeader}>{m.add()}</Button>
            </div>
            {#each headers as header, index (index)}
              <div class="flex flex-wrap items-center gap-2">
                <Input class="w-40" value={header.name} placeholder={m.plugin_header_name()} oninput={event => setHeader(index, 'name', event.currentTarget.value)} />
                <Input class="w-52" type="password" value={header.value} placeholder={m.plugin_header_value()} oninput={event => setHeader(index, 'value', event.currentTarget.value)} />
                <Button size="xs" variant="ghost" class="text-error" onclick={() => removeHeader(index)}>{m.remove()}</Button>
              </div>
            {/each}
          </div>
          {#if previewError}<p class="text-xs text-error" role="alert">{previewError}</p>{/if}
          <Button size="sm" variant="primary" loading={store.loading} onclick={() => void preview()}>{m.plugin_preview()}</Button>
          {#if store.wasmSchemaPreview !== null}
            <Textarea readonly rows={12} class="font-mono text-xs" value={formatJson(store.wasmSchemaPreview)} data-label={m.plugin_preview_result()} />
          {/if}
        </div>
      {/if}
      {#if store.wasmSchema !== null}
        <Textarea readonly rows={12} class="font-mono text-xs" value={formatJson(store.wasmSchema)} data-label={m.plugin_saved_schema()} />
      {/if}
    </section>

    {#if store.error}<p class="text-sm text-error" role="alert">{store.error}</p>{/if}

    {#if deleting}
      <div class="rounded-xl border border-error/30 bg-error/5 p-3">
        <p class="text-sm">{m.plugin_delete_confirm({ name: displayName || instanceName || uid })}</p>
        <div class="mt-3 flex justify-end gap-2">
          <Button variant="ghost" size="sm" disabled={store.saving} onclick={() => { deleting = false }}>{m.cancel()}</Button>
          <Button variant="error" size="sm" loading={store.saving} onclick={() => void confirmDelete()}>{m.delete()}</Button>
        </div>
      </div>
    {/if}

    </div>

    <div class="flex shrink-0 justify-end gap-2 border-t border-base-content/8 pt-3">
      <Button variant="ghost" disabled={store.saving} onclick={() => { open = false }}>{m.cancel()}</Button>
      <Button variant="primary" loading={store.saving} disabled={kind === 'mono'} onclick={() => void save()} data-testid="plugin-save">{m.save()}</Button>
    </div>
  </div>
</Drawer>
