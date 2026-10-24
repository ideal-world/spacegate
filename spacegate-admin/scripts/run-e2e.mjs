/** 运行浏览器验收，保留本轮证据；退出后仅回收本轮服务数据，失败和中断同样处理。 */
import { spawn } from 'node:child_process'
import { mkdirSync, mkdtempSync, rmSync } from 'node:fs'
import { createRequire } from 'node:module'
import { tmpdir } from 'node:os'
import { join, resolve } from 'node:path'
import process from 'node:process'

const cwd = process.cwd()
const require = createRequire(resolve(cwd, 'package.json'))
const cli = require.resolve('@playwright/test/cli')
const dataParent = resolve(process.env.HAI_E2E_DATA_ROOT || tmpdir())
mkdirSync(dataParent, { recursive: true })
const dataRoot = mkdtempSync(join(dataParent, 'hai-browser-e2e-'))
const groups = [[]]
for (const argument of process.argv.slice(2)) {
  if (argument === '--next')
    groups.push([])
  else
    groups.at(-1).push(argument)
}
let child
let interrupted = false
let stopping
/**
 * 结束本轮 Playwright；Windows 同时结束其进程树，finally 等待该命令退出后才清理数据。
 * @returns {void} 终止请求由 child 退出事件和 stopping Promise 确认。
 * @example 模块内部信号处理
 * ```js
 * process.once('SIGTERM', stop)
 * ```
 */
function stop() {
  if (interrupted)
    return
  interrupted = true
  if (process.platform === 'win32' && child?.pid && child.exitCode === null) {
    // Windows 的 kill 不传递 SIGTERM；同时结束本次 Playwright 创建的浏览器与服务进程。
    stopping = new Promise((resolve) => {
      const killer = spawn('taskkill', ['/pid', String(child.pid), '/t', '/f'], { windowsHide: true, stdio: 'ignore' })
      killer.once('error', () => {
        child?.kill()
        resolve()
      })
      killer.once('exit', resolve)
    })
  }
  else {
    child?.kill()
  }
}
process.once('SIGINT', stop)
process.once('SIGTERM', stop)
try {
  // 证据与临时业务数据分开；每组使用独立输出，后续 Playwright 不会清空前组证据。
  const evidenceParent = resolve(process.env.HAI_AUTH_EVIDENCE_DIR || join(cwd, 'test-results'))
  mkdirSync(evidenceParent, { recursive: true })
  const runRoot = mkdtempSync(join(evidenceParent, 'run-'))
  for (const [index, args] of groups.entries()) {
    const groupRoot = join(runRoot, `group-${index + 1}`)
    const groupDataRoot = join(dataRoot, `group-${index + 1}`)
    mkdirSync(groupDataRoot)
    const hasOutput = args.some(argument => argument === '--output' || argument.startsWith('--output='))
    const outputArgs = hasOutput ? [] : ['--output', join(groupRoot, 'results')]
    const html = process.env.PLAYWRIGHT_HTML_OUTPUT_DIR || process.env.PLAYWRIGHT_HTML_REPORT
    const code = await new Promise((resolve, reject) => {
      child = spawn(process.execPath, [cli, 'test', ...args, ...outputArgs], {
        cwd,
        windowsHide: true,
        stdio: 'inherit',
        env: {
          ...process.env,
          ...(!html ? { PLAYWRIGHT_HTML_OUTPUT_DIR: join(groupRoot, 'html') } : {}),
          HAI_AUTH_EVIDENCE_DIR: process.env.HAI_AUTH_EVIDENCE_DIR || join(groupRoot, 'evidence'),
          HAI_E2E_DATA_ROOT: groupDataRoot,
        },
      })
      child.once('error', reject)
      child.once('exit', resolve)
    })
    // 配置组互相独立，普通失败继续收集；保留首个失败码，中断才停止后续组。
    if (code !== 0)
      process.exitCode ||= typeof code === 'number' && code !== 0 ? code : 1
    if (interrupted) {
      process.exitCode ||= 1
      break
    }
  }
}
finally {
  process.removeListener('SIGINT', stop)
  process.removeListener('SIGTERM', stop)
  await stopping
  // Playwright 退出后服务进程已关闭，此时删除 SQLite / storage，避免 Windows 文件锁。
  try {
    rmSync(dataRoot, { recursive: true, force: true, maxRetries: 3, retryDelay: 100 })
  }
  catch (error) {
    process.stderr.write(`E2E data cleanup failed: ${error instanceof Error ? error.message : String(error)}\n`)
    // 清理错误不能把原始失败（如 exit 7）覆盖成另一状态，也不能让成功变为假成功。
    if (!process.exitCode)
      process.exitCode = 1
  }
}
