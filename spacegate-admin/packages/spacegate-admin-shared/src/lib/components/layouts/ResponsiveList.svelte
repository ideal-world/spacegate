<!--
  @component ResponsiveList
  同一份数据同时提供桌面表格与移动卡片两种呈现，统一空态与断点。

  桌面（≥md）渲染 `desktop` 插槽，通常是 DataTable；移动端按 `keyOf` 逐项渲染
  `mobile` 插槽，通常是 Card。数据为空时渲染 `empty` 插槽。断点只在组件内部切换，
  两套呈现不会同时显示，避免重复的可点击元素干扰键盘与验收脚本。

  @example
  ```svelte
  <ResponsiveList items={rows} keyOf={row => row.name}>
    {#snippet desktop()}<DataTable data={rows} {columns} keyField="name" />{/snippet}
    {#snippet mobile(row)}<Card>{row.name}</Card>{/snippet}
  </ResponsiveList>
  ```
-->
<script lang="ts" generics="T">
  import type { Snippet } from 'svelte'

  interface Props {
    /** 当前列表数据；空数组时渲染空态。 */
    items: T[]
    /** 桌面端整体呈现（表格），由页面把 items 传入具体表格组件。 */
    desktop: Snippet
    /** 移动端单条记录的呈现。 */
    mobile: Snippet<[T]>
    /** 记录唯一键，用于 `{#each}` 的 keyed 渲染。 */
    keyOf: (item: T) => string
    /** 空态内容；页面通常放 Empty 组件与主操作。 */
    empty?: Snippet
    /** 移动端卡片容器类名，默认双列网格。 */
    mobileClass?: string
  }
  const {
    items,
    desktop,
    mobile,
    keyOf,
    empty,
    mobileClass = 'grid content-start gap-3 p-3 sm:grid-cols-2 sm:p-4',
  }: Props = $props()
</script>

{#if items.length === 0}
  {#if empty}<div class="p-6">{@render empty()}</div>{/if}
{:else}
  <div class="hidden min-h-0 flex-1 overflow-auto md:block">{@render desktop()}</div>
  <div class="min-h-0 flex-1 overflow-y-auto md:hidden {mobileClass}">
    {#each items as item (keyOf(item))}{@render mobile(item)}{/each}
  </div>
{/if}
