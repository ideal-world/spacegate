<!--
  @component PluginPicker
  右侧插件选择抽屉：先选插件类型（原生 / Wasm），再引用已有实例或新建实例。

  与旧管理端的“选择插件 + 引用已有配置 / 新建配置”对应，但只保留一层选择：
  插件类型 → 实例（已有或新建），不再为不同执行位置重复配置。

  @example
  ```svelte
  <PluginPicker bind:open store={store} onpick={(result) => result.type === 'instance' ? bind(result.instance) : create(result.code)} />
  ```
-->
<script lang="ts">
  import type { ManagementStore } from '../../stores/management-store.svelte.js'
  import type { PluginInstance, PluginPick } from '../../spacegate/model.js'
  import { Drawer, Empty, Tabs } from '@h-ai/ui/components/compounds/index'
  import { Badge, Button, Input } from '@h-ai/ui/components/primitives/index'
  import * as m from '../../paraglide/messages.js'
    import {
    asRecord,
    isWasmPluginCode,
    isWasmPluginDefinition,
    pluginKey,
    pluginReferenceName,
    readString,
  } from '../../spacegate/model.js'

  interface Props {
    store: ManagementStore
    open?: boolean
    /** 插件实例白名单；缺省时使用 store.plugins。 */
    instances?: PluginInstance[]
    /** 仅允许 Wasm 扩展（例如插件中心的 Wasm 页签）。 */
    wasmOnly?: boolean
    /** 是否允许“新建实例”入口。 */
    allowCreate?: boolean
    onpick: (pick: PluginPick) => void
  }
  let { store, open = $bindable(false), instances, wasmOnly = false, allowCreate = true, onpick }: Props = $props()

  /** 原生插件卡片：从 upstream attr JSON 提取展示字段。 */
  interface PluginCard {
    code: string
    title: string
    description: string
    version: string
    mono: boolean
  }

  let activeTab = $state<'native' | 'wasm'>('native')
  let search = $state('')

  const tabItems = $derived(wasmOnly
    ? [{ key: 'wasm', label: m.plugin_tab_wasm() }]
    : [{ key: 'native', label: m.plugin_tab_native() }, { key: 'wasm', label: m.plugin_tab_wasm() }])

  const pool = $derived(instances ?? store.plugins)

  const nativeCards = $derived(store.pluginAttrs
    .map((attr): PluginCard => {
      const meta = asRecord(attr.meta) ?? {}
      const code = readString(attr, 'code')
      return {
        code,
        title: readString(meta, 'title') || code,
        description: readString(meta, 'description'),
        version: readString(meta, 'version'),
        mono: attr.mono === true,
      }
    })
    .filter(card => !isWasmPluginCode(card.code))
    .filter(matches))

  const wasmInstances = $derived(pool.filter(isWasmPluginDefinition).filter(matches))

  const nativeInstances = $derived(pool.filter(instance => !isWasmPluginCode(instance.ref.code)).filter(matches))

  /** 卡片与实例共用同一个搜索词，匹配代码、名称和说明。 */
  function matches(item: PluginCard | PluginInstance): boolean {
    const term = search.trim().toLowerCase()
    if (!term)
      return true
    if ('ref' in item) {
      const title = instanceTitle(item)
      return item.ref.code.toLowerCase().includes(term)
        || title.toLowerCase().includes(term)
        || pluginReferenceName(item.ref).toLowerCase().includes(term)
    }
    return item.code.toLowerCase().includes(term)
      || item.title.toLowerCase().includes(term)
      || item.description.toLowerCase().includes(term)
  }

  function instanceTitle(instance: PluginInstance): string {
    return instance.displayName
      || readString(instance.spec, 'title')
      || readString(instance.spec, 'plugin_name')
      || pluginReferenceName(instance.ref)
  }

  function instancesOf(code: string): PluginInstance[] {
    return nativeInstances.filter(instance => instance.ref.code === code)
  }

  function pickInstance(instance: PluginInstance) {
    onpick({ type: 'instance', instance })
    open = false
  }

  function createFor(code: string) {
    onpick({ type: 'plugin', code })
    open = false
  }
</script>

<Drawer bind:open title={m.plugin_picker_title()} position="right" size="3xl" closeOnBackdrop={!store.saving}>
  <div class="flex h-full min-h-0 flex-col gap-3">
    <p class="text-xs text-base-content/50">{m.plugin_picker_desc()}</p>

    {#if !wasmOnly}
      <Tabs items={tabItems} active={activeTab} onchange={(key) => { activeTab = key === 'wasm' ? 'wasm' : 'native' }} />
    {/if}

    <Input type="search" bind:value={search} placeholder={m.plugin_search_hint()} />

    <div class="min-h-0 flex-1 space-y-3 overflow-y-auto pb-2">
      {#if activeTab === 'native'}
        {#if nativeCards.length === 0}
          <Empty size="sm" title={m.plugin_catalog_empty()}>
            {#snippet action()}
              <Button size="sm" variant="outline" loading={store.loading} onclick={() => void store.refreshPlugins()}>{m.plugin_refresh()}</Button>
            {/snippet}
          </Empty>
        {:else}
          {#each nativeCards as card (card.code)}
            {@const existing = instancesOf(card.code)}
            <div class="rounded-xl border border-base-content/8 bg-base-100 p-3" data-testid="plugin-pick-card">
              <div class="flex items-start gap-3">
                <span class="grid size-9 shrink-0 place-items-center rounded-xl bg-primary/10 text-primary" aria-hidden="true"><span class="icon-[tabler--puzzle] size-4.5"></span></span>
                <div class="min-w-0 flex-1">
                  <p class="line-clamp-2 text-sm font-semibold">{card.title}</p>
                  <p class="truncate font-mono text-xs text-base-content/45">{card.code}{card.version ? ` · v${card.version}` : ''}</p>
                  {#if card.description}<p class="mt-1 line-clamp-2 text-xs leading-5 text-base-content/55">{card.description}</p>{/if}
                </div>
                {#if card.mono}<Badge size="sm" variant="ghost">{m.plugin_kind_mono()}</Badge>{/if}
              </div>

              {#if existing.length > 0}
                <div class="mt-3 space-y-1.5 border-t border-base-content/8 pt-2">
                  <p class="text-xs text-base-content/45">{m.plugin_reference_existing()}</p>
                  {#each existing as instance (pluginKey(instance.ref))}
                    <button type="button" class="flex w-full items-center gap-2 rounded-lg px-2 py-1.5 text-left text-sm hover:bg-base-200/60" onclick={() => pickInstance(instance)}>
                      <span class="min-w-0 flex-1 truncate">{instanceTitle(instance)}</span>
                      <span class="truncate font-mono text-xs text-base-content/40">{pluginReferenceName(instance.ref)}</span>
                      <span class="text-xs text-primary">{m.plugin_use()}</span>
                    </button>
                  {/each}
                </div>
              {/if}

              {#if allowCreate && !card.mono}
                <div class="mt-2 flex justify-end">
                  <Button size="xs" variant="ghost" onclick={() => createFor(card.code)}>{m.plugin_create_new()}</Button>
                </div>
              {/if}
            </div>
          {/each}
        {/if}
      {:else}
        <div class="flex items-center justify-between gap-2">
          <p class="text-xs text-base-content/50">{m.plugin_wasm_desc()}</p>
          {#if allowCreate}<Button size="xs" variant="outline" onclick={() => createFor('wasm')}>{m.plugin_wasm_add()}</Button>{/if}
        </div>
        {#if wasmInstances.length === 0}
          <Empty size="sm" title={m.plugin_wasm_empty()} description={m.plugin_wasm_bind_hint()} />
        {:else}
          {#each wasmInstances as instance (pluginKey(instance.ref))}
            <div class="flex items-center gap-3 rounded-xl border border-base-content/8 bg-base-100 p-3" data-testid="plugin-pick-card">
              <span class="grid size-9 shrink-0 place-items-center rounded-xl bg-accent/15 text-accent-content" aria-hidden="true"><span class="icon-[tabler--bolt] size-4.5"></span></span>
              <div class="min-w-0 flex-1">
                <p class="line-clamp-2 text-sm font-semibold">{instanceTitle(instance)}</p>
                <p class="truncate font-mono text-xs text-base-content/45">{instance.ref.code} · {pluginReferenceName(instance.ref)}</p>
              </div>
              <Button size="xs" variant="outline" onclick={() => pickInstance(instance)}>{m.plugin_use()}</Button>
            </div>
          {/each}
        {/if}
      {/if}
    </div>
  </div>
</Drawer>
