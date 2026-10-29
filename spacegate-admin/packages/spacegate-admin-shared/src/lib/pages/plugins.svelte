<script lang="ts">
  import type { ManagementStore } from '../stores/management-store.svelte.js'
  import type { PluginInstance } from '../spacegate/model.js'
  import { Card, Empty, Modal, Skeleton, Tabs } from '@h-ai/ui/components/compounds/index'
  import { Badge, BareButton, Button, Input } from '@h-ai/ui/components/primitives/index'
  import * as m from '../paraglide/messages.js'
  import {
    asRecord,
    asRecordArray,
    bindingReferenceOf,
    isWasmPluginCode,
    isWasmPluginDefinition,
    pluginIdOf,
    pluginKey,
    pluginReferenceName,
    readString,
  } from '../spacegate/model.js'
  import PluginEditor from '../components/spacegate/PluginEditor.svelte'
  import ListPageLayout from '../components/layouts/ListPageLayout.svelte'

  interface Props { store: ManagementStore }
  const { store }: Props = $props()

  /** 插件目录卡片：从 upstream attr JSON 提取展示字段。 */
  interface PluginCard {
    code: string
    title: string
    description: string
    version: string
    mono: boolean
    authors: string
    homepage: string
    repository: string
  }

  let activeTab = $state<'native' | 'wasm'>('native')
  let search = $state('')
  let detailCode = $state('')
  let instanceSearch = $state('')
  let editorOpen = $state(false)
  let editorKey = $state<string | null>(null)
  let deleting = $state<PluginInstance | null>(null)
  let editorOpened = $state(false)
  let deleteError = $state('')

  const tabItems = $derived([
    { key: 'native', label: m.plugin_tab_native() },
    { key: 'wasm', label: m.plugin_tab_wasm() },
  ])

  const cards = $derived(store.pluginAttrs.map((attr): PluginCard => {
    const meta = asRecord(attr.meta) ?? {}
    const code = readString(attr, 'code')
    const authors = meta.authors
    return {
      code,
      title: readString(meta, 'title') || code,
      description: readString(meta, 'description'),
      version: readString(meta, 'version'),
      mono: attr.mono === true,
      authors: Array.isArray(authors) ? authors.filter((item): item is string => typeof item === 'string').join(', ') : readString(meta, 'authors'),
      homepage: readString(meta, 'homepage'),
      repository: readString(meta, 'repository'),
    }
  }))

  const nativeCards = $derived(cards
    .filter(card => !isWasmPluginCode(card.code))
    .filter((card) => {
      const term = search.trim().toLowerCase()
      return !term || card.code.toLowerCase().includes(term) || card.title.toLowerCase().includes(term) || card.description.toLowerCase().includes(term)
    }))

  const wasmPlugins = $derived(store.plugins
    .filter(isWasmPluginDefinition)
    .filter((instance) => {
      const term = search.trim().toLowerCase()
      if (!term)
        return true
      const title = wasmTitle(instance)
      return instance.ref.code.toLowerCase().includes(term)
        || title.toLowerCase().includes(term)
        || readString(instance.spec, 'description').toLowerCase().includes(term)
    }))

  const detailAttr = $derived(cards.find(card => card.code === detailCode))
  const detailInstances = $derived(store.plugins.filter(instance => instance.ref.code === detailCode).filter((instance) => {
    const term = instanceSearch.trim().toLowerCase()
    if (!term)
      return true
    return (instance.displayName || pluginReferenceName(instance.ref)).toLowerCase().includes(term)
      || pluginReferenceName(instance.ref).toLowerCase().includes(term)
  }))
  const gatewayBindings = $derived(asRecordArray(asRecord(store.gatewayConfig)?.plugins))

  /** 编辑器保存或删除后会按 code 收窄实例列表，关闭时重新拉取全部实例，保持两个页签数据完整。 */
  $effect(() => {
    if (editorOpen) {
      editorOpened = true
      return
    }
    if (editorOpened) {
      editorOpened = false
      void store.loadPlugins()
    }
  })

  function kindLabel(kind: string): string {
    if (kind === 'named')
      return m.plugin_kind_named()
    if (kind === 'anon')
      return m.plugin_kind_anon()
    return m.plugin_kind_mono()
  }

  function wasmTitle(instance: PluginInstance): string {
    return instance.displayName
      || readString(instance.spec, 'title')
      || readString(instance.spec, 'plugin_name')
      || pluginReferenceName(instance.ref)
  }

  function enabledOnGateway(instance: PluginInstance): boolean {
    if (instance.ref.kind !== 'named')
      return false
    const target = instance.ref
    return gatewayBindings.some((item) => {
      const ref = bindingReferenceOf(item)
      return ref !== null && ref.kind === 'named' && ref.code === target.code && ref.name === target.name
    })
  }

  function switchTab(next: string) {
    activeTab = next === 'wasm' ? 'wasm' : 'native'
    search = ''
    detailCode = ''
  }

  function selectPlugin(code: string) {
    detailCode = code
    instanceSearch = ''
  }

  async function backToList() {
    detailCode = ''
    await store.loadPlugins()
  }

  function openCreate(code?: string) {
    if (code)
      store.selectedPluginCode = code
    editorKey = null
    editorOpen = true
  }

  function openEdit(instance: PluginInstance) {
    store.selectedPluginCode = instance.ref.code
    editorKey = pluginKey(instance.ref)
    editorOpen = true
  }

  async function confirmDelete() {
    if (!deleting)
      return
    deleteError = ''
    const id = pluginIdOf(deleting.ref)
    if (!id)
      return
    const deleted = await store.deletePlugin(id)
    if (deleted) {
      deleting = null
      await store.loadPlugins()
    }
    else {
      deleteError = store.error || m.operation_failed()
    }
  }
</script>

<ListPageLayout title={m.nav_plugins()} description={m.plugins_description()} testid="plugins-page">
  {#snippet actions()}
    <Button variant="outline" size="sm" loading={store.loading} onclick={() => void store.refreshPlugins()}>{m.plugin_refresh()}</Button>
  {/snippet}

  {#if detailCode}
    <Card bordered class="min-h-0 flex-1 overflow-hidden">
      <div class="flex flex-wrap items-center gap-3">
        <Button variant="ghost" size="sm" onclick={() => void backToList()}>
          <span class="icon-[tabler--arrow-left] size-4" aria-hidden="true"></span>{m.plugin_detail_back()}
        </Button>
        <span class="font-mono text-sm text-base-content/70">{detailCode}</span>
        {#if detailAttr?.version}<Badge size="sm" variant="ghost">v{detailAttr.version}</Badge>{/if}
        {#if detailAttr?.mono}<Badge size="sm" variant="ghost">{m.plugin_kind_mono()}</Badge>{/if}
      </div>

      {#if detailAttr}
        <div class="mt-4 space-y-5">
          <div class="flex flex-col gap-1">
            {#if detailAttr.title}<h2 class="text-lg font-semibold tracking-tight">{detailAttr.title}</h2>{/if}
            {#if detailAttr.description}<p class="text-sm leading-6 text-base-content/60">{detailAttr.description}</p>{/if}
          </div>

          <dl class="grid gap-x-6 gap-y-2 rounded-xl border border-base-content/8 p-4 text-sm sm:grid-cols-[8rem_minmax(0,1fr)]">
            <dt class="text-base-content/45">{m.plugin_meta_type()}</dt><dd class="font-mono text-xs">{detailAttr.code}</dd>
            <dt class="text-base-content/45">{m.plugin_meta_version()}</dt><dd>{detailAttr.version || '—'}</dd>
            <dt class="text-base-content/45">{m.plugin_meta_mono()}</dt><dd>{detailAttr.mono ? m.yes() : m.no()}</dd>
            {#if detailAttr.authors}
              <dt class="text-base-content/45">{m.plugin_meta_authors()}</dt><dd>{detailAttr.authors}</dd>
            {/if}
            {#if detailAttr.homepage}
              <dt class="text-base-content/45">{m.plugin_meta_homepage()}</dt><dd class="break-all"><a class="link link-primary" href={detailAttr.homepage} target="_blank" rel="noreferrer">{detailAttr.homepage}</a></dd>
            {/if}
            {#if detailAttr.repository}
              <dt class="text-base-content/45">{m.plugin_meta_repository()}</dt><dd class="break-all"><a class="link link-primary" href={detailAttr.repository} target="_blank" rel="noreferrer">{detailAttr.repository}</a></dd>
            {/if}
          </dl>

          {#if detailAttr.mono}
            <p class="rounded-lg bg-base-200/50 p-3 text-sm text-base-content/60">{m.plugin_mono_readonly()}</p>
          {:else}
            <section class="space-y-3">
              <div class="flex flex-wrap items-end justify-between gap-3">
                <div>
                  <h3 class="text-sm font-semibold">{m.plugin_instance_title()}</h3>
                  <p class="mt-0.5 text-xs text-base-content/50">{m.plugin_instance_desc()}</p>
                </div>
                <div class="flex flex-wrap items-center gap-2">
                  <div class="w-56"><Input type="search" bind:value={instanceSearch} placeholder={m.plugin_instance_search()} /></div>
                  <Button size="sm" variant="primary" onclick={() => openCreate(detailCode)}>{m.plugin_create()}</Button>
                </div>
              </div>

              {#if detailInstances.length === 0}
                <div class="rounded-xl border border-base-content/8 p-4">
                  <Empty size="sm" title={m.plugin_instance_empty()} />
                </div>
              {:else}
                <div class="divide-y divide-base-content/8 overflow-hidden rounded-xl border border-base-content/8">
                  {#each detailInstances as instance (pluginKey(instance.ref))}
                    <div class="flex flex-wrap items-center gap-3 px-4 py-3">
                      <div class="min-w-0 flex-1">
                        <p class="truncate text-sm font-medium">{instance.displayName || pluginReferenceName(instance.ref)}</p>
                        <p class="truncate font-mono text-xs text-base-content/45">{instance.ref.code} / {kindLabel(instance.ref.kind)}</p>
                      </div>
                      <Button size="xs" variant="outline" onclick={() => openEdit(instance)}>{m.edit()}</Button>
                      <Button size="xs" variant="ghost" class="text-error" onclick={() => { deleting = instance }}>{m.delete()}</Button>
                    </div>
                  {/each}
                </div>
              {/if}
            </section>
          {/if}
        </div>
      {:else if store.loading}
        <div class="mt-4"><Skeleton count={4} /></div>
      {/if}
    </Card>
  {:else}
    <Card bordered class="min-h-0 flex-1 overflow-hidden">
      <div class="flex flex-col gap-4">
        <div class="flex flex-wrap items-start justify-between gap-3">
          <div>
            <h2 class="text-base font-semibold">{m.plugin_market()}</h2>
            <p class="mt-1 text-sm text-base-content/50">{m.plugin_market_desc()}</p>
          </div>
        </div>

        <Tabs items={tabItems} active={activeTab} onchange={switchTab} />
        <div class="w-full sm:w-72">
          <Input type="search" bind:value={search} placeholder={m.plugin_search_hint()} />
        </div>

        {#if store.loading && cards.length === 0}
          <div class="grid gap-3 sm:grid-cols-2 xl:grid-cols-4"><Skeleton count={6} /></div>
        {:else if activeTab === 'native'}
          {#if nativeCards.length === 0}
            <Empty title={m.plugin_catalog_empty()} description={m.plugins_description()} />
          {:else}
            <div class="grid gap-3 sm:grid-cols-2 xl:grid-cols-4">
              {#each nativeCards as card (card.code)}
                <div class="flex flex-col overflow-hidden rounded-xl border border-base-content/8 bg-base-100 transition-colors hover:border-primary/40" data-testid="plugin-card">
                  <div class="flex flex-1 flex-col gap-3 p-4">
                    <div class="flex items-start gap-3">
                      <span class="grid size-10 shrink-0 place-items-center rounded-xl bg-primary/10 text-primary" aria-hidden="true"><span class="icon-[tabler--puzzle] size-5"></span></span>
                      <div class="min-w-0 flex-1">
                        <p class="line-clamp-2 text-sm font-semibold">{card.title}</p>
                        <p class="truncate font-mono text-xs text-base-content/45">{card.code}{card.version ? ` · v${card.version}` : ''}</p>
                      </div>
                      {#if card.mono}<Badge size="sm" variant="ghost">{m.plugin_kind_mono()}</Badge>{/if}
                    </div>
                    <p class="line-clamp-3 min-h-10 text-xs leading-5 text-base-content/55">{card.description}</p>
                  </div>
                  <BareButton class="w-full border-t border-base-content/8 px-4 py-2 text-center text-xs font-medium text-primary hover:bg-primary/5" onclick={() => selectPlugin(card.code)}>{m.plugin_configure()}</BareButton>
                </div>
              {/each}
            </div>
          {/if}
        {:else}
          <section class="space-y-3">
            <div class="flex flex-wrap items-end justify-between gap-3">
              <div>
                <h3 class="text-sm font-semibold">{m.plugin_tab_wasm()}</h3>
                <p class="mt-0.5 text-xs text-base-content/50">{m.plugin_wasm_desc()}</p>
              </div>
              <Button size="sm" variant="primary" onclick={() => openCreate('wasm')}>{m.plugin_wasm_add()}</Button>
            </div>

            {#if wasmPlugins.length === 0}
              <div class="rounded-xl border border-base-content/8 p-4">
                <Empty size="sm" title={m.plugin_wasm_empty()} description={m.plugin_wasm_bind_hint()} />
              </div>
            {:else}
              <div class="grid gap-3 sm:grid-cols-2 xl:grid-cols-4">
                {#each wasmPlugins as instance (pluginKey(instance.ref))}
                  <div class="flex flex-col overflow-hidden rounded-xl border border-base-content/8 bg-base-100" data-testid="plugin-card">
                    <div class="flex flex-1 flex-col gap-3 p-4">
                      <div class="flex items-start gap-3">
                        <span class="grid size-10 shrink-0 place-items-center rounded-xl bg-accent/15 text-accent-content" aria-hidden="true"><span class="icon-[tabler--bolt] size-5"></span></span>
                        <div class="min-w-0 flex-1">
                          <p class="line-clamp-2 text-sm font-semibold">{wasmTitle(instance)}</p>
                          <p class="truncate font-mono text-xs text-base-content/45">{instance.ref.code} · {pluginReferenceName(instance.ref)}</p>
                        </div>
                      </div>
                      <p class="line-clamp-3 min-h-10 text-xs leading-5 text-base-content/55">{readString(instance.spec, 'description') || m.no_data()}</p>
                      <Badge size="sm" variant={enabledOnGateway(instance) ? 'success' : 'ghost'}>{enabledOnGateway(instance) ? m.plugin_wasm_enabled() : m.plugin_wasm_disabled()}</Badge>
                    </div>
                    <div class="flex items-center justify-between gap-2 border-t border-base-content/8 px-4 py-2">
                      <span class="truncate text-xs text-base-content/45">{m.plugin_wasm_bind_hint()}</span>
                      <div class="flex shrink-0 items-center gap-1">
                        <Button size="xs" variant="ghost" onclick={() => openEdit(instance)}>{m.edit()}</Button>
                        <Button size="xs" variant="ghost" class="text-error" onclick={() => { deleting = instance }}>{m.delete()}</Button>
                      </div>
                    </div>
                  </div>
                {/each}
              </div>
            {/if}
          </section>
        {/if}
      </div>
    </Card>
  {/if}
</ListPageLayout>

<PluginEditor bind:open={editorOpen} instanceKey={editorKey} {store} />

<Modal open={!!deleting} title={m.delete()} onclose={() => { deleting = null }} closeOnBackdrop={!store.saving} closeOnEscape={!store.saving}>
  <p class="text-sm leading-6 text-base-content/70">{m.plugin_delete_confirm({ name: deleting ? (deleting.displayName || pluginReferenceName(deleting.ref)) : '' })}</p>
  {#if deleteError}<p class="mt-3 text-sm text-error" role="alert">{deleteError}</p>{/if}
  {#snippet footer()}
    <Button variant="ghost" disabled={store.saving} onclick={() => { deleting = null }}>{m.cancel()}</Button>
    <Button variant="error" loading={store.saving} onclick={() => void confirmDelete()}>{m.delete()}</Button>
  {/snippet}
</Modal>
