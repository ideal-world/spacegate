<script lang="ts">
  import type { ManagementStore } from '../stores/management-store.svelte.js'
  import type { PluginInstance } from '../spacegate/model.js'
  import { Card, Empty, PageHeader, Skeleton } from '@h-ai/ui/components/compounds/index'
  import { Badge, Button, Input, Select } from '@h-ai/ui/components/primitives/index'
  import * as m from '../paraglide/messages.js'
  import { asRecord, pluginKey, pluginReferenceName, readString } from '../spacegate/model.js'
  import PluginEditor from '../components/spacegate/PluginEditor.svelte'

  interface Props { store: ManagementStore }
  const { store }: Props = $props()

  let editorOpen = $state(false)
  let editorKey = $state<string | null>(null)
  let search = $state('')

  const codeOptions = $derived([{ value: '', label: m.plugin_all_codes() }, ...store.pluginCatalog.map(code => ({ value: code, label: code }))])
  const instances = $derived(store.plugins.filter((instance) => {
    const term = search.trim().toLowerCase()
    if (!term)
      return true
    return instance.ref.code.toLowerCase().includes(term)
      || instance.displayName.toLowerCase().includes(term)
      || pluginReferenceName(instance.ref).toLowerCase().includes(term)
  }))
  const attrCards = $derived(store.pluginAttrs.map((attr) => {
    const meta = asRecord(attr.meta) ?? {}
    return {
      code: readString(attr, 'code'),
      title: readString(meta, 'title') || readString(attr, 'code'),
      description: readString(meta, 'description'),
      version: readString(meta, 'version'),
      mono: attr.mono === true,
    }
  }).filter(attr => !attr.code.startsWith('wasm')))

  function selectCode(value: string) {
    if (value) {
      void store.loadPlugins(value)
}
    else {
      store.selectedPluginCode = ''
      void store.loadPlugins()
    }
  }

  function openCreate(code?: string) {
    if (code)
      store.selectedPluginCode = code
    editorKey = null
    editorOpen = true
  }

  function openEdit(instance: PluginInstance) {
    editorKey = pluginKey(instance.ref)
    editorOpen = true
  }
</script>

<section class="space-y-5" data-testid="plugins-page">
  <PageHeader title={m.nav_plugins()} description={m.plugins_description()}>
    {#snippet actions()}
      <div class="flex flex-wrap gap-2">
        <Button variant="outline" size="sm" loading={store.loading} onclick={() => void store.refreshPlugins()}>{m.plugin_refresh()}</Button>
        <Button variant="primary" size="sm" onclick={() => openCreate()} data-testid="plugin-create">{m.plugin_create()}</Button>
      </div>
    {/snippet}
  </PageHeader>

  <Card title={m.plugin_catalog()} bordered shadow={false}>
    {#if attrCards.length === 0}
      <p class="text-sm text-base-content/50">{m.plugin_catalog_empty()}</p>
    {:else}
      <div class="grid gap-2 sm:grid-cols-2 xl:grid-cols-3">
        {#each attrCards as attr (attr.code)}
          <div class="flex flex-wrap items-center justify-between gap-2 rounded-lg border border-base-content/8 px-3 py-2">
            <div class="min-w-0">
              <p class="truncate text-sm font-medium">{attr.title}</p>
              <p class="truncate font-mono text-xs text-base-content/45">{attr.code}{attr.version ? ` · ${attr.version}` : ''}</p>
            </div>
            <div class="flex items-center gap-1">
              {#if attr.mono}<Badge size="sm" variant="ghost">{m.plugin_kind_mono()}</Badge>{/if}
              <Button size="xs" variant="ghost" onclick={() => openCreate(attr.code)}>{m.plugin_create()}</Button>
            </div>
          </div>
        {/each}
      </div>
    {/if}
  </Card>

  <div class="rounded-xl border border-base-content/8 bg-base-100">
    <div class="flex flex-wrap items-center gap-3 border-b border-base-content/8 p-4">
      <div class="w-56">
        <Select value={store.selectedPluginCode} options={codeOptions} placeholder={m.plugin_all_codes()} onchange={selectCode} />
      </div>
      <div class="min-w-48 flex-1">
        <Input type="search" bind:value={search} placeholder={m.plugins_search()} />
      </div>
    </div>
    {#if store.loading && store.plugins.length === 0}
      <div class="p-4"><Skeleton count={4} /></div>
    {:else if instances.length === 0}
      <div class="p-6">
        <Empty title={m.plugin_no_instances()} description={m.plugins_description()}>
          {#snippet action()}<Button size="sm" variant="primary" onclick={() => openCreate(store.selectedPluginCode || undefined)}>{m.plugin_create()}</Button>{/snippet}
        </Empty>
      </div>
    {:else}
      <div class="grid gap-3 p-4 sm:grid-cols-2 xl:grid-cols-3">
        {#each instances as instance (pluginKey(instance.ref))}
          <Card bordered shadow={false}>
            <div class="flex items-start justify-between gap-2">
              <div class="min-w-0">
                <p class="truncate font-medium">{instance.displayName || pluginReferenceName(instance.ref)}</p>
                <p class="truncate font-mono text-xs text-base-content/45">{instance.ref.code}</p>
              </div>
              <Badge size="sm" variant="ghost">{instance.ref.kind}</Badge>
            </div>
            <p class="mt-2 break-all text-xs text-base-content/55">{pluginReferenceName(instance.ref)}</p>
            <div class="mt-3 flex flex-wrap gap-1">
              <Button size="xs" variant="outline" onclick={() => openEdit(instance)}>{m.edit()}</Button>
            </div>
          </Card>
        {/each}
      </div>
    {/if}
  </div>
</section>

<PluginEditor bind:open={editorOpen} instanceKey={editorKey} {store} />
