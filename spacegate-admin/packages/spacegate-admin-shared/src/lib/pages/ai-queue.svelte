<script lang="ts">
  import type { ManagementStore } from '../stores/management-store.svelte.js'
  import type { JsonRecord } from '../spacegate/model.js'
  import { Card, Empty } from '@h-ai/ui/components/compounds/index'
  import { Button, Input, Select, Textarea } from '@h-ai/ui/components/primitives/index'
  import * as m from '../paraglide/messages.js'
  import { formatJson, readNumber, readString } from '../spacegate/model.js'
  import AiQueueRuleEditor from '../components/spacegate/AiQueueRuleEditor.svelte'
  import DataSurface from '../components/layouts/DataSurface.svelte'
  import ListPageLayout from '../components/layouts/ListPageLayout.svelte'

  interface Props { store: ManagementStore }
  const { store }: Props = $props()

  let editorOpen = $state(false)
  let editing = $state<JsonRecord | null>(null)
  let deleting = $state<JsonRecord | null>(null)
  let tenant = $state('')
  let model = $state('')
  let path = $state('')
  let policy = $state('')
  let resolveTenant = $state('')
  let resolveModel = $state('')
  let resolvePath = $state('')
  let resolvePolicy = $state('')
  let resolveError = $state('')

  const policyOptions = [{ value: '', label: m.ai_policy_any() }, { value: 'abandon', label: 'abandon' }, { value: 'queue', label: 'queue' }, { value: 'wait', label: 'wait' }]

  function filters() {
    return {
      tenant: tenant.trim() || undefined,
      model: model.trim() || undefined,
      path: path.trim() || undefined,
      policy: policy ? policy as 'abandon' | 'queue' | 'wait' : undefined,
    }
  }

  function resetFilters() {
    tenant = ''
    model = ''
    path = ''
    policy = ''
    void store.loadAiQueue()
  }

  function openCreate() {
    editing = null
    editorOpen = true
  }

  function openEdit(rule: JsonRecord) {
    editing = rule
    editorOpen = true
  }

  async function confirmDelete() {
    if (deleting && await store.deleteAiRule(deleting))
      deleting = null
  }

  async function resolve() {
    resolveError = ''
    if (!resolveTenant.trim()) {
      resolveError = m.ai_validation_tenant()
      return
    }
    await store.resolveAiRule({
      tenant: resolveTenant.trim(),
      model: resolveModel.trim() || undefined,
      path: resolvePath.trim() || undefined,
      policy: resolvePolicy ? resolvePolicy as 'abandon' | 'queue' | 'wait' : undefined,
    })
  }
</script>

<ListPageLayout title={m.nav_ai_queue()} description={m.ai_queue_description()} testid="ai-queue-page">
  {#snippet actions()}
    <Button variant="outline" size="sm" loading={store.loading} onclick={() => void store.loadAiQueue(filters())}>{m.home_refresh()}</Button>
    <Button variant="primary" size="sm" onclick={openCreate} data-testid="ai-rule-create">{m.ai_rule_create()}</Button>
  {/snippet}

  <Card title={m.ai_filters()} bordered>
    <div class="grid gap-3 sm:grid-cols-2 xl:grid-cols-4">
      <Input bind:value={tenant} placeholder={m.ai_tenant()} />
      <Input bind:value={model} placeholder={m.ai_model()} />
      <Input bind:value={path} placeholder={m.ai_path()} />
      <Select bind:value={policy} options={policyOptions} />
    </div>
    <div class="mt-3 flex flex-wrap gap-2">
      <Button size="sm" variant="outline" loading={store.loading} onclick={() => void store.loadAiQueue(filters())}>{m.search()}</Button>
      <Button size="sm" variant="ghost" onclick={resetFilters}>{m.reset()}</Button>
    </div>
  </Card>

  <DataSurface>
    {#snippet toolbar()}
      <h2 class="text-sm font-semibold">{m.ai_rules()}</h2>
      <span class="text-xs text-base-content/45">{store.aiRules.length}</span>
    {/snippet}
    {#if store.aiRules.length === 0}
      <div class="p-6"><Empty title={m.ai_no_rules()} description={m.ai_queue_description()} /></div>
    {:else}
      <div class="overflow-auto">
        <table class="table w-full text-sm">
          <thead>
            <tr class="text-left text-xs text-base-content/50">
              <th class="px-3 py-2">{m.ai_tenant()}</th>
              <th class="px-3 py-2">{m.ai_model()}</th>
              <th class="px-3 py-2">{m.ai_path()}</th>
              <th class="px-3 py-2">{m.ai_policy()}</th>
              <th class="px-3 py-2">{m.ai_rps()}</th>
              <th class="px-3 py-2">{m.ai_burst()}</th>
              <th class="px-3 py-2">{m.ai_cost()}</th>
              <th class="px-3 py-2">{m.ai_ttl()}</th>
              <th class="px-3 py-2">{m.ai_key()}</th>
              <th class="px-3 py-2">{m.actions()}</th>
            </tr>
          </thead>
          <tbody>
            {#each store.aiRules as rule, index (readString(rule, 'key') || String(index))}
              <tr class="border-t border-base-content/8 align-top">
                <td class="px-3 py-2 font-medium">{readString(rule, 'tenant')}</td>
                <td class="px-3 py-2">{readString(rule, 'model') || '—'}</td>
                <td class="px-3 py-2 font-mono text-xs">{readString(rule, 'path') || '—'}</td>
                <td class="px-3 py-2">{readString(rule, 'policy') || '—'}</td>
                <td class="px-3 py-2">{readNumber(rule, 'rps') ?? '—'}</td>
                <td class="px-3 py-2">{readNumber(rule, 'burst') ?? '—'}</td>
                <td class="px-3 py-2">{readNumber(rule, 'cost') ?? '—'}</td>
                <td class="px-3 py-2">{readNumber(rule, 'ttl_secs') ?? m.ai_ttl_forever()}</td>
                <td class="max-w-52 break-all px-3 py-2 font-mono text-xs text-base-content/50">{readString(rule, 'key') || '—'}</td>
                <td class="px-3 py-2">
                  <div class="flex flex-wrap gap-1">
                    <Button size="xs" variant="ghost" onclick={() => openEdit(rule)}>{m.edit()}</Button>
                    <Button size="xs" variant="ghost" class="text-error" onclick={() => { deleting = rule }}>{m.delete()}</Button>
                  </div>
                </td>
              </tr>
            {/each}
          </tbody>
        </table>
      </div>
    {/if}
    {#if deleting}
      <div class="border-t border-error/20 bg-error/5 p-4">
        <p class="text-sm">{m.ai_rule_delete_confirm({ name: readString(deleting, 'key') || readString(deleting, 'tenant') })}</p>
        <div class="mt-3 flex gap-2">
          <Button size="sm" variant="ghost" disabled={store.saving} onclick={() => { deleting = null }}>{m.cancel()}</Button>
          <Button size="sm" variant="error" loading={store.saving} onclick={() => void confirmDelete()}>{m.delete()}</Button>
        </div>
      </div>
    {/if}
  </DataSurface>

  <div class="grid gap-5 xl:grid-cols-2">
    <Card title={m.ai_resolve()} bordered>
      <div class="grid gap-3 sm:grid-cols-2">
        <Input bind:value={resolveTenant} placeholder={m.ai_tenant()} />
        <Input bind:value={resolveModel} placeholder={m.ai_model()} />
        <Input bind:value={resolvePath} placeholder={m.ai_path()} />
        <Select bind:value={resolvePolicy} options={policyOptions} />
      </div>
      {#if resolveError}<p class="mt-2 text-sm text-error" role="alert">{resolveError}</p>{/if}
      <Button class="mt-3" size="sm" variant="outline" loading={store.loading} onclick={() => void resolve()}>{m.ai_resolve()}</Button>
      {#if store.aiResolution !== null}
        <div class="mt-4">
          <p class="mb-1 text-xs font-medium text-base-content/55">{m.ai_resolution()}</p>
          <Textarea readonly rows={12} class="font-mono text-xs" value={formatJson(store.aiResolution)} />
        </div>
      {/if}
    </Card>

    <Card title={m.ai_plugin_schema()} bordered>
      <Textarea readonly rows={12} class="font-mono text-xs" value={formatJson(store.aiSchema)} />
    </Card>
  </div>

  <Card title={m.ai_plugin_readme()} bordered>
    {#if store.aiReadme}
      <pre class="max-h-96 overflow-auto whitespace-pre-wrap break-words rounded-lg bg-base-200/40 p-3 text-xs">{store.aiReadme}</pre>
    {:else}
      <p class="text-sm text-base-content/50">{m.no_data()}</p>
    {/if}
  </Card>
</ListPageLayout>

<AiQueueRuleEditor bind:open={editorOpen} rule={editing} {store} />
