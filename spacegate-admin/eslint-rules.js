/**
 * 应用共享质量门禁。依赖 Antfu 已注册的 TypeScript / ESLint comments 插件。
 * 必要第三方 workaround 使用单行 eslint-disable-next-line <规则> -- 具体原因，
 * 不关闭整个文件的检查。秘密泄漏、输入/权限校验仍需结合业务审查。
 */
export const haiQualityConfig = {
  files: ['**/*.{js,mjs,cjs,ts,tsx,svelte}'],
  linterOptions: {
    reportUnusedDisableDirectives: 'error',
  },
  rules: {
    'no-console': ['error', {}],
    'ts/no-explicit-any': 'error',
    'ts/ban-ts-comment': ['error', {
      'ts-ignore': true,
      'ts-nocheck': true,
      'ts-check': false,
      'ts-expect-error': 'allow-with-description',
      'minimumDescriptionLength': 10,
    }],
    'eslint-comments/no-unlimited-disable': 'error',
    'eslint-comments/require-description': 'error',
    // 只禁内部源码/构建目录，公开的 /client、/adapter 等 exports 子路径可用。
    'no-restricted-imports': ['error', {
      patterns: [{
        group: ['@h-ai/*/src', '@h-ai/*/src/**', '@h-ai/*/dist', '@h-ai/*/dist/**'],
        message: 'Use the public exports of hai packages instead of their src/dist internals.',
      }],
    }],
    'no-restricted-syntax': ['error', {
      selector: ':matches(TSAsExpression, TSTypeAssertion)[expression.typeAnnotation.type="TSUnknownKeyword"]',
      message: 'Use the real upstream type or narrow unknown. Explain necessary third-party workarounds with a scoped lint exception.',
    }, {
      selector: 'ImportExpression[source.value=/^@h-ai\\/[^/]+\\/(src|dist)(\\/|$)/]',
      message: 'Use the public exports of hai packages instead of their src/dist internals.',
    }, {
      // 仅识别明确的静态敏感键；变量键、别名及业务敏感程度仍需审查。
      selector: 'CallExpression[callee.property.name="setItem"][callee.object.name="localStorage"][arguments.0.value=/^(token|password|secret|api[_-]?key|(access|refresh|auth|session)[_-]?token)$/i]',
      message: 'Keep sensitive tokens in memory, httpOnly cookies or a secure native TokenStore instead of localStorage.',
    }, {
      selector: 'CallExpression[callee.property.name="setItem"][callee.object.property.name="localStorage"][arguments.0.value=/^(token|password|secret|api[_-]?key|(access|refresh|auth|session)[_-]?token)$/i]',
      message: 'Keep sensitive tokens in memory, httpOnly cookies or a secure native TokenStore instead of localStorage.',
    }],
  },
}
