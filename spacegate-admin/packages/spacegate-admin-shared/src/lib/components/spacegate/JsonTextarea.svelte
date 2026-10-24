<script lang="ts">
  import { Button, Textarea } from '@h-ai/ui/components/primitives/index'
  import * as m from '../../paraglide/messages.js'
  import { formatJson, parseJson } from '../../spacegate/model.js'

  interface Props {
    value: unknown
    onchange: (value: unknown) => void
    rows?: number
    label?: string
    disabled?: boolean
    testid?: string
  }
  const { value, onchange, rows = 16, label, disabled = false, testid }: Props = $props()

  let synced = $state('')
  let text = $state('')
  let error = $state('')
  let copied = $state(false)

  // 外部模型变化时同步文本；用户刚刚输入且语义相同的文本不会被重新格式化，避免光标跳动。
  $effect(() => {
    const next = formatJson(value)
    if (next !== synced) {
      synced = next
      text = next
      error = ''
    }
  })

  function input(event: Event & { currentTarget: HTMLTextAreaElement }) {
    text = event.currentTarget.value
    copied = false
    const parsed = parseJson(text)
    if (!parsed.ok) {
      error = m.json_invalid()
      return
    }
    error = ''
    synced = formatJson(parsed.value)
    onchange(parsed.value)
  }

  function format() {
    const parsed = parseJson(text)
    if (!parsed.ok) {
      error = m.json_invalid()
      return
    }
    text = formatJson(parsed.value)
    error = ''
    synced = text
    onchange(parsed.value)
  }

  async function copy() {
    if (typeof navigator === 'undefined' || !navigator.clipboard)
      return
    try {
      await navigator.clipboard.writeText(text)
      copied = true
    }
    catch {
      copied = false
    }
  }
</script>

<div class="space-y-2" data-testid={testid}>
  <div class="flex flex-wrap items-center justify-between gap-2">
    {#if label}<span class="text-sm font-medium">{label}</span>{/if}
    <div class="ml-auto flex items-center gap-1">
      <Button size="xs" variant="ghost" disabled={disabled} onclick={format}>{m.json_format()}</Button>
      <Button size="xs" variant="ghost" disabled={disabled} onclick={() => void copy()}>{copied ? m.json_copied() : m.json_copy()}</Button>
    </div>
  </div>
  <Textarea value={text} rows={rows} disabled={disabled} class="font-mono text-xs" oninput={input} data-label={label ?? m.json_editor()} />
  {#if error}<p class="text-xs text-error" role="alert">{error}</p>{/if}
  <p class="text-xs text-base-content/50">{m.json_import_hint()}</p>
</div>
