# spacegate-admin-shared

## Introduction

Shared mini administration workspace for **spacegate-admin**. Web, Capacitor App and Tauri desktop use the same `AdminWorkspace`, responsive `AppShell`, typed API client and management store.

## Core features

- Shared management pages, forms, navigation and application state.
- Typed encrypted API client and platform-specific token storage.
- Themes, language preferences, onboarding and help.

## Getting started

### Behavior and configuration

The behavior below describes the initial example. Roles, permissions, workflows, limits, preferences and audit policies can change with the user's requirements. Update conflicting implementation, tests and guidance together; preserve the applicable public API contracts and server security checks.

- Administrators manage users, upload user avatars, assign the built-in administrator/user roles, edit role descriptions and query audit logs.
- By default, ordinary users update their own profile, avatar, language, appearance and password. The server enforces the actual business permissions on every request.
- The workspace uses the available window width. Tables scroll within their own area; settings panels form two columns on large screens and stack on phones.
- `ManagementStore` owns API calls and business state; components render the state and forward user interactions.
- Application name, version, environment and default locale come from the backend `app.info` contract, which reads `core` configuration. Do not duplicate these values in translations or frontend constants.
- Requests, token refresh and JSON responses use the same `@h-ai/api-client` + `@h-ai/crypto` transport. Avatar uploads use encrypted JSON and validate PNG/JPEG/WebP content, with a 512 KiB limit.
- Read metadata through the same initialized `managementApi.app.info()` client; inspect `success` before using `data`.

Vite development and preview servers read the workspace root environment files and proxy /api to the backend. Builds do not load deployment environment files. Web containers receive their public API address at startup; native apps follow the platform configuration below.

An origin-only API base uses `/api/v1`. For a custom service `http.apiPrefix`, include that path in `PUBLIC_API_BASE` or the runtime API base; the client preserves the configured path and restricts avatar URLs to its matching resource prefix.

### Authentication and preferences

Web and browser previews keep access tokens in memory and refresh tokens in the server's httpOnly Cookie. Requests include credentials; reload restores the session through the encrypted refresh flow. Never store authentication tokens in localStorage.

Capacitor native uses `createCapacitorTokenStorage()` and the trusted native token channel. Set its public API address in the App's `public/runtime-config.js` before compiling and syncing native assets. Tauri uses memory token storage and requires a new login after restart; set process `PUBLIC_API_BASE` before launching the installed application, and its Rust host injects the address. Both send `clientName` and `X-HAI-Token-Transport: body`; the service verifies the trusted native origin before returning a refresh token in JSON. Missing API configuration is an error, not a successful connection.

Language and theme preferences are saved in IAM user metadata, restored after login and synchronized with the shared/host Paraglide runtimes and `core.i18n`. The small reactive theme store delegates DOM updates and safe appearance caching to `@h-ai/ui/theme` (`applyTheme` / `getSavedTheme`); the framework uses the `theme` localStorage key. Appearance selectors live in personal settings and save through the authenticated profile contract.

### Onboarding and help

AdminWorkspace mounts the shared `@h-ai/ui/guide` controller and GuideHost after authentication. AppShell includes a help button; first-use invitations offer start, later and dismiss. Web stores preferences in Cookies, while App/Desktop use localStorage, scoped by the current user and tour version. Help articles link to the overview and profile tours.

Extend `src/lib/stores/guide-store.ts` and the shared Chinese/English/Japanese messages, then add stable data-guide attributes to existing components. Navigation uses the same ManagementStore. Keep business text as i18n callbacks and preserve the host's locale/scope binding. Browser tests cover completion, reload, replay, settings navigation and mobile help.

### i18n and themes

Shared page copy belongs in `messages/{zh-CN,en-US,ja-JP}.json`. Frontend-specific copy belongs in each app's own messages directory. Keep `baseLocale` and `locales` aligned across all enabled projects, then run root `pnpm i18n:compile`; do not edit generated Paraglide files.

All frontend and shared projects load the installed message-format plugin from the workspace root `node_modules`, matching the scaffold's hoisted dependency layout. Compilation uses the workspace dependency version and does not fetch a plugin from a CDN.

The scaffold theme configuration determines the themes compiled into frontend CSS. Personal settings offer only those available themes. Customize the common layout in `src/lib/components/AppShell.svelte`, the management pages in adjacent components, and business behavior in `src/lib/stores/management-store.svelte.ts`.

### Validation

Run root `pnpm typecheck`, `pnpm lint`, `pnpm build`, `pnpm test` and `pnpm test:e2e`. Shared API tests exercise real transport key exchange, encrypted request/response handling, configured core metadata and credential forwarding. Root browser tests cover administrator and ordinary-user workflows, avatar rendering, persisted preferences, password/session invalidation and narrow-screen layout. All selected Web/App/Desktop previews also exercise login, tours, help and profile navigation. Clean screenshots, reports, temporary projects, containers and helper processes in finally/teardown, including failure paths. These browser checks do not replace native-shell or miniapp device acceptance.

### IAM configuration

The service's `config/_iam.yml` controls login methods, registration, password reset, agreements and `ui` branding. Email and SMS use providers/templates in `_reach.yml` or custom callbacks. `AuthPanel` uses the public configuration and framework auth actions to manage tokens, resend countdowns and errors. A single process can use memory cache. Multiple instances need shared database/cache, write routing and a shared managed transport key ring and registry as described in the project README.

Authentication defaults to a split layout. Set `ui.variant`, `title`, `subtitle`, `brandTitle`, `brandText`, `logoUrl` and `illustrationUrl` to customize login and registration. Without a `brandTitle`, the displayed application name comes from `app.info` and follows `core` configuration.

LDAP and Reach are optional. Install ldapts in the IAM service workspace only when using its default LDAP client, then enable login.ldap and set connection settings. Custom onOtpSendEmail/onOtpSendSms/onPasswordResetRequest callbacks work without Reach and must reject on delivery failure. Their third context argument supplies expiresAt/idempotencyKey; senders durably deduplicate that key, and errors/timeouts mean unknown delivery with no automatic resend. Agreements open in a Modal. LoginForm/RegisterForm support header/extraFields/footer snippets; AuthPanel forwards loginHeader/loginFields/loginFooter and registerHeader/registerFields/registerFooter. See the generated hai-iam/hai-ui skills for configuration and examples.

Agreements accept Markdown text or a URL. Markdown takes precedence and opens in a safely rendered dialog.

See `.agents/skills/hai-iam/reference.md` in the generated application for the complete contract.

### Form layouts

The user editor uses `FormLayout columns={2} labelPosition="left"`. Personal settings fill the available width and use two columns on wide screens, with stacked labels and a single avatar editor. Narrow containers automatically stack cards, fields and labels. UI controls and CRUD forms show client validation on blur, update errors while correcting input and validate untouched fields on submit. Field errors follow the current UI language. The management store syncs locale through the lightweight `@h-ai/ui/i18n` entry, without loading components or tour CSS. Configure `colSpan` on FormField to override automatic spans for textareas, uploads and multiple choices.

## Development and testing

Run root `pnpm typecheck`, `pnpm lint`, `pnpm build`, `pnpm test` and `pnpm test:e2e`. Browser checks cover shared workflows; test native shells on their target platforms separately.

## 界面设计与验收

共享管理界面的 Web/App/Desktop 使用同一套 UI 组件和状态。设计、实现、审查分别参考根目录 `.agents/skills/hai-ui-design/`、`hai-ui/`、`hai-ui-review/`；正常、加载、空态、错误恢复、键盘、语言/主题和窄容器均按实际场景验收。组件的筛选、排序和分页仍由业务数据回调接入。


## License

Apache-2.0
