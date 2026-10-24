import type { HaiResult } from '@h-ai/core'
import { capacitor } from '@h-ai/capacitor'

export async function initCapacitor(): Promise<HaiResult<void>> {
  const result = await capacitor.init()
  if (!result.success)
    return result

  if (capacitor.isNative()) {
    return capacitor.statusBar.configure({
      style: 'dark',
      overlay: true,
    })
  }
  return result
}
