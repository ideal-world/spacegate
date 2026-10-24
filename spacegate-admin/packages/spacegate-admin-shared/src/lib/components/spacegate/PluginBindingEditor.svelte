<script lang="ts">
  import type { PluginInstance } from '../../spacegate/model.js'
  import { Button, Input, Select } from '@h-ai/ui/components/primitives/index'
  import * as m from '../../paraglide/messages.js'
  import { asRecord, bindingPriority, bindingReferenceOf, createBinding, pluginKey, pluginReferenceName } from '../../spacegate/model.js'

  interface Props {
    bindings?: unknown
    instances: PluginInstance[]
    disabled?: boolean
    onchange: (bindings: unknown[]) => void
  }
  const { bindings, instances, disabled = false, onchange }: Props = $props()
  let selectedKey = $state('')
  let priority = $state(0)

  const list = $derived(Array.isArray(bindings) ? bindings : [])
  const rows = $derived(list.map((item, index) => {
    const reference = bindingReferenceOf(item)
    const instance = reference ? instances.find(entry => pluginKey(entry.ref) === pluginKey(reference)) : undefined
    return { index, reference, priority: bindingPriority(item), label: instance?.displayName || (reference ? pluginReferenceName(reference) : '') }
  }))
  const options = $derived(instances.map(instance => ({
    value: pluginKey(instance.ref),
    label: instance.displayName ? `${instance.displayName} (${pluginReferenceName(instance.ref)})` : pluginReferenceName(instance.ref),
  })))

  function update(index: number, next: Record<string, unknown>) {
    const items = [...list]
    items[index] = next
    onchange(items)
  }

  function remove(index: number) {
    onchange(list.filter((_, itemIndex) => itemIndex !== index))
  }

  function move(index: number, delta: number) {
    const target = index + delta
    if (target < 0 || target >= list.length)
      return
    const items = [...list]
    const [item] = items.splice(index, 1)
    items.splice(target, 0, item)
    onchange(items)
  }

  function add() {
    const instance = instances.find(item => pluginKey(item.ref) === selectedKey)
    if (!instance)
      return
    onchange([...list, createBinding(instance.ref, priority)])
    priority = 0
  }
</script>

<div class="space-y-3" data-testid="plugin-bindings">
  {#if rows.length === 0}
    <p class="text-xs text-base-content/45">{m.binding_empty()}</p>
  {:else}
    <div class="space-y-2">
      {#each rows as row (row.index)}
        <div class="flex flex-wrap items-center gap-2 rounded-lg border border-base-content/10 p-2">
          <span class="min-w-0 flex-1 truncate text-sm">{row.label || row.reference?.code || '—'}</span>
          <span class="text-xs text-base-content/45">{row.reference?.code}</span>
          <label class="flex items-center gap-1 text-xs text-base-content/60">
            {m.binding_priority()}
            <Input
              type="number"
              step={1}
              class="w-24"
              value={String(row.priority)}
              {disabled}
              onchange={(event) => {
                const record = asRecord(list[row.index])
                if (record)
                  update(row.index, { ...record, priority: Number(event.currentTarget.value) || 0 })
              }}
            />
          </label>
          <Button size="xs" variant="ghost" {disabled} onclick={() => move(row.index, -1)} ariaLabel={m.move_up()}>↑</Button>
          <Button size="xs" variant="ghost" {disabled} onclick={() => move(row.index, 1)} ariaLabel={m.move_down()}>↓</Button>
          <Button size="xs" variant="ghost" class="text-error" {disabled} onclick={() => remove(row.index)}>{m.remove()}</Button>
        </div>
      {/each}
    </div>
  {/if}
  <div class="flex flex-wrap items-end gap-2 border-t border-base-content/8 pt-3">
    <div class="min-w-48 flex-1">
      <Select bind:value={selectedKey} options={options} placeholder={m.binding_select_instance()} {disabled} />
    </div>
    <label class="flex items-center gap-1 text-xs text-base-content/60">
      {m.binding_priority()}
      <Input type="number" step={1} class="w-24" value={String(priority)} {disabled} oninput={(event) => { priority = Number(event.currentTarget.value) || 0 }} />
    </label>
    <Button size="sm" variant="outline" {disabled} onclick={add}>{m.binding_add()}</Button>
  </div>
  <p class="text-xs text-base-content/45">{m.binding_order_hint()}</p>
</div>
