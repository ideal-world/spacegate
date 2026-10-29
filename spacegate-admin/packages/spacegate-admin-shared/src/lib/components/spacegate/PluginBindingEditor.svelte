<script lang="ts">
  import type { PluginInstance } from '../../spacegate/model.js'
  import { Badge, Button, Input } from '@h-ai/ui/components/primitives/index'
  import * as m from '../../paraglide/messages.js'
  import { asRecord, bindingPriority, bindingReferenceOf, pluginKey, pluginReferenceName, readString } from '../../spacegate/model.js'

  interface Props {
    bindings?: unknown
    instances: PluginInstance[]
    disabled?: boolean
    /** 请求打开插件选择器；选择器由页面渲染，避免嵌套在抽屉内部时定位错乱。 */
    onrequestPick?: () => void
    onchange: (bindings: unknown[]) => void
  }
  const { bindings, instances, disabled = false, onrequestPick, onchange }: Props = $props()

  const list = $derived(Array.isArray(bindings) ? bindings : [])
  const rows = $derived(list.map((item, index) => {
    const reference = bindingReferenceOf(item)
    const instance = reference ? instances.find(entry => pluginKey(entry.ref) === pluginKey(reference)) : undefined
    return {
      index,
      reference,
      priority: bindingPriority(item),
      title: instance?.displayName || (reference ? pluginReferenceName(reference) : ''),
      description: instance ? readString(instance.spec, 'description') : '',
    }
  }))

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
</script>

<div class="space-y-3" data-testid="plugin-bindings">
  {#if rows.length === 0}
    <div class="rounded-xl border border-dashed border-base-content/15 px-4 py-6 text-center">
      <p class="text-sm text-base-content/50">{m.binding_empty()}</p>
    </div>
  {:else}
    <div class="divide-y divide-base-content/8 overflow-hidden rounded-xl border border-base-content/8">
      {#each rows as row (row.index)}
        <div class="flex flex-wrap items-center gap-2 px-3 py-2.5">
          <span class="grid size-8 shrink-0 place-items-center rounded-lg bg-primary/10 text-primary" aria-hidden="true"><span class="icon-[tabler--puzzle] size-4"></span></span>
          <div class="min-w-0 flex-1">
            <p class="truncate text-sm font-medium">{row.title || row.reference?.code || '—'}</p>
            <p class="truncate font-mono text-xs text-base-content/45">{row.reference?.code}{row.description ? ` · ${row.description}` : ''}</p>
          </div>
          <Badge size="sm" variant="ghost">{m.binding_priority()} {row.priority}</Badge>
          <label class="flex items-center gap-1 text-xs text-base-content/60">
            <span class="sr-only">{m.binding_priority()}</span>
            <Input
              type="number"
              step={1}
              class="w-20"
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

  <div class="flex flex-wrap items-center gap-2">
    <Button size="sm" variant="outline" {disabled} onclick={() => onrequestPick?.()} data-testid="plugin-bind">
      <span class="icon-[tabler--plus] size-4" aria-hidden="true"></span>{m.binding_add()}
    </Button>
    <span class="text-xs text-base-content/45">{m.binding_order_hint()}</span>
  </div>
</div>
