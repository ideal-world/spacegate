<!--
  @component ListPageLayout
  列表型管理页的统一骨架：页头（标题/描述/主操作）、可选工具条、内容区、可选页脚（分页）。

  页面只提供业务内容，间距、滚动和断点行为由此组件统一，避免每个页面各写一套
  header + 容器 + 分页组合。内容区默认不滚动，需要固定表格高度的页面可以再传
  `contentClass`，由表格自身承接滚动。

  @example
  ```svelte
  <ListPageLayout title="网关" description="维护网关监听器" testid="gateways-page">
    {#snippet actions()}<Button>新建</Button>{/snippet}
    {#snippet toolbar()}<Input placeholder="搜索" />{/snippet}
    <DataSurface>…</DataSurface>
  </ListPageLayout>
  ```
-->
<script lang="ts">
  import type { Snippet } from 'svelte'
  import { PageHeader } from '@h-ai/ui/components/compounds/index'

  interface Props {
    /** 页面标题，显示在页头左侧。 */
    title: string
    /** 页面说明，显示在标题下方。 */
    description?: string
    /** 页面根节点的 data-testid，供验收脚本定位整页。 */
    testid?: string
    /** 页头右侧按钮区（刷新、新建等主操作）。 */
    actions?: Snippet
    /** 页头与内容之间的工具条（搜索、筛选）。 */
    toolbar?: Snippet
    /** 页面底部固定区（分页、批量操作）。 */
    footer?: Snippet
    /** 内容区附加类名，例如需要整块滚动时传 `min-h-0 flex-1`。 */
    contentClass?: string
    children: Snippet
  }
  const { title, description, testid, actions: headerActions, toolbar, footer, contentClass = '', children }: Props = $props()
</script>

<section class="flex min-h-0 w-full min-w-0 flex-1 flex-col gap-4" data-testid={testid}>
  <PageHeader class="mb-0" {title} {description}>
    {#snippet actions()}
      {#if headerActions}{@render headerActions()}{/if}
    {/snippet}
  </PageHeader>
  {#if toolbar}
    <div class="flex flex-wrap items-center gap-3">{@render toolbar()}</div>
  {/if}
  <div class="flex min-w-0 flex-col gap-4 {contentClass}">
    {@render children()}
  </div>
  {#if footer}
    <div class="shrink-0">{@render footer()}</div>
  {/if}
</section>
