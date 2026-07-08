//! 验证 host 端 VmPool + 后台 tick 任务能正确驱动 `proxy_on_tick`。
//!
//! - `on_vm_start`：设置 50ms 的 tick 周期；
//! - `on_tick`：把全局 tick 计数器（shared_data，key="on_tick.count"）原子地 +1；
//!
//! 测试侧通过 `spacegate_plugin_wasm::shared::shared_data_get` 直接读 shared_data，
//! 等若干 tick 之后断言计数大于 0。
//!
//! `set_shared_data` 用 cas-loop 保证多 VM / 后台并发也不会丢更新（虽然现在只有一条 tick 任务）。

use std::time::Duration;

use proxy_wasm::hostcalls;
use proxy_wasm::traits::*;
use proxy_wasm::types::*;

const KEY: &str = "on_tick.count";

proxy_wasm::main! {{
    proxy_wasm::set_log_level(LogLevel::Info);
    proxy_wasm::set_root_context(|_| -> Box<dyn RootContext> { Box::new(TickRoot) });
}}

struct TickRoot;
impl Context for TickRoot {}
impl RootContext for TickRoot {
    fn on_vm_start(&mut self, _: usize) -> bool {
        // 50ms 周期：host 端默认 50ms 颗粒度的轮询正好能驱动。
        let _ = hostcalls::set_tick_period(Duration::from_millis(50));
        true
    }

    fn on_tick(&mut self) {
        // cas 循环：读 → +1 → 写；写失败 (CasMismatch) 重读重试。
        for _ in 0..8 {
            let (cur, cas) = hostcalls::get_shared_data(KEY).unwrap_or((None, None));
            let next = cur.as_deref().and_then(|b| std::str::from_utf8(b).ok()).and_then(|s| s.parse::<u64>().ok()).unwrap_or(0) + 1;
            let buf = next.to_string();
            match hostcalls::set_shared_data(KEY, Some(buf.as_bytes()), cas) {
                Ok(()) => return,
                Err(Status::CasMismatch) => continue,
                Err(_) => return,
            }
        }
    }
}
