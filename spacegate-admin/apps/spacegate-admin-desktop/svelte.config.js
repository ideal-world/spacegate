import { autoImportHaiUi } from '@h-ai/ui/auto-import'
import { vitePreprocess } from '@sveltejs/vite-plugin-svelte'

/** @type {import('@sveltejs/vite-plugin-svelte').SvelteConfig} */
export default {
  preprocess: [autoImportHaiUi(), vitePreprocess()],
  compilerOptions: {
    // 使用 Svelte 5 Runes，和 @h-ai/ui 组件库保持一致。
    runes: true,
  },
}
