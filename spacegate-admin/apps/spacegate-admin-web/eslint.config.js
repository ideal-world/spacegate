import antfu, { GLOB_SRC } from '@antfu/eslint-config'
import { haiQualityConfig } from '../../eslint-rules.js'

export default antfu({
  // unused-imports 使用 TS 规则，JS 同样使用 TS parser 以保持 scope 契约一致。
  typescript: { files: [GLOB_SRC] },
  formatters: true,
  ignores: ['**/build/**', '**/dist/**', '**/*.d.ts', '**/project.inlang/**', '**/src/lib/paraglide/**'],
  svelte: true,
}, haiQualityConfig, {
  files: ['**/*.svelte'],
  rules: {
    'svelte/html-quotes': 'off',
    'svelte/indent': 'off',
    'perfectionist/sort-imports': 'off',
  },
})
