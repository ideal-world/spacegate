<!--
  @component DataSurface
  列表/表格的统一承载容器：圆角边框、背景、工具条分隔线与页脚。

  工具条放在 header 插槽（通常放搜索与筛选），页脚插槽通常放分页；
  内容宽度由表格自身控制，容器负责裁剪圆角并承接纵向滚动。

  @example
  ```svelte
  <DataSurface>
    {#snippet toolbar()}<Input type="search" bind:value={search} />{/snippet}
    <DataTable data={rows} {columns} keyField="name" />
    {#snippet footer()}<Pagination page={1} total={40} pageSize={20} onchange={() => {}} />{/snippet}
  </DataSurface>
  ```
-->
<script lang="ts">
  import type { Snippet } from 'svelte'

  interface Props {
    /** 顶部工具条（搜索、筛选、批量动作）。 */
    toolbar?: Snippet
    /** 底部操作区（分页、统计）。 */
    footer?: Snippet
    /** 附加类名，例如填满剩余高度时传 `min-h-0 flex-1`。 */
    class?: string
    children: Snippet
  }
  const { toolbar, footer, class: className = '', children }: Props = $props()
</script>

<div class="flex min-h-0 min-w-0 flex-col overflow-hidden rounded-xl border border-base-content/8 bg-base-100 {className}">
  {#if toolbar}
    <div class="flex shrink-0 flex-wrap items-center gap-3 border-b border-base-content/8 p-3 sm:p-4">{@render toolbar()}</div>
  {/if}
  {@render children()}
  {#if footer}
    <div class="shrink-0 border-t border-base-content/8 p-3 sm:p-4">{@render footer()}</div>
  {/if}
</div>
