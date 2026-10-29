<!--
  @component StatCard
  概览页指标卡：标签、图标、数值、补充说明与可选行动入口。

  数值节点单独接收 `valueTestId`，保证验收脚本对数值断言不会混入标签文案。

  @example
  ```svelte
  <StatCard label="网关" value={3} icon="icon-[tabler--network]" valueTestId="overview-gateways" />
  ```
-->
<script lang="ts">
  import type { Snippet } from 'svelte'

  interface Props {
    /** 指标名称。 */
    label: string
    /** 指标数值，已格式化好的字符串或数字。 */
    value: string | number
    /** 右上角图标类名（如 icon-[tabler--network]）。 */
    icon?: string
    /** 数值下方的补充说明，例如在线实例比例。 */
    hint?: string
    /** 数值节点的 data-testid。 */
    valueTestId?: string
    /** 数值下方的行动入口（跳转或刷新）。 */
    action?: Snippet
  }
  const { label, value, icon, hint, valueTestId, action }: Props = $props()
</script>

<div class="rounded-xl border border-base-content/8 bg-base-100 p-4 transition-shadow hover:shadow-sm">
  <div class="flex items-start justify-between gap-2">
    <p class="text-xs text-base-content/55">{label}</p>
    {#if icon}
      <span class="grid size-8 shrink-0 place-items-center rounded-lg bg-primary/10 text-primary" aria-hidden="true"><span class="{icon} size-4"></span></span>
    {/if}
  </div>
  <p class="mt-2 text-2xl font-semibold tabular-nums" data-testid={valueTestId}>{value}</p>
  {#if hint}<p class="mt-1 text-xs text-base-content/45">{hint}</p>{/if}
  {#if action}<div class="mt-3">{@render action()}</div>{/if}
</div>
