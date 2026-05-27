//! 端到端验证 `proxy_set_tick_period_milliseconds` + `proxy_on_tick` ⇄ host VmPool
//! 后台 tick 任务的协同：
//!
//! 1. `WasmPluginShell::create` 后，shell 内部起一条 50ms 颗粒度的 tick 循环；
//! 2. guest 在 `on_vm_start` 把 tick 周期设为 50ms，`on_tick` 把 shared_data 计数原子 +1；
//! 3. 测试 sleep 几个 tick 周期后从 host 侧直接读 shared_data，断言至少 N 次 tick；
//! 4. `drop(shell)` 后再 sleep，确认计数不再继续增长（tick 任务随 shell 析构）。

use std::path::PathBuf;
use std::time::Duration;

use spacegate_model::{PluginInstanceId, PluginInstanceName};
use spacegate_plugin::{Plugin, PluginConfig};
use spacegate_plugin_wasm::shared::{shared_data_get, shared_data_set};
use spacegate_plugin_wasm::WasmPluginShell;

// ─────────────────────────────────────────────────────────
// guest .wasm 定位/构建
// ─────────────────────────────────────────────────────────

fn guest_manifest_path() -> PathBuf {
    let mut p = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    p.push("tests");
    p.push("on_tick_guest");
    p.push("Cargo.toml");
    p
}

fn guest_wasm_path() -> PathBuf {
    let manifest = guest_manifest_path();
    let out = std::process::Command::new(env!("CARGO"))
        .args(["metadata", "--no-deps", "--format-version", "1", "--manifest-path"])
        .arg(&manifest)
        .output()
        .expect("cargo metadata: spawn");
    assert!(out.status.success(), "cargo metadata failed: {}", String::from_utf8_lossy(&out.stderr));
    let meta: serde_json::Value = serde_json::from_slice(&out.stdout).expect("parse cargo metadata json");
    let target_dir = meta["target_directory"].as_str().expect("target_directory missing");
    PathBuf::from(target_dir).join("wasm32-wasip1").join("release").join("on_tick_guest.wasm")
}

fn ensure_guest_built() -> PathBuf {
    let wasm = guest_wasm_path();
    if !wasm.exists() {
        let status = std::process::Command::new(env!("CARGO"))
            .args(["build", "--release", "--target", "wasm32-wasip1", "--manifest-path"])
            .arg(guest_manifest_path())
            .status()
            .expect("cargo build: spawn");
        assert!(status.success(), "on_tick_guest build failed");
        assert!(wasm.exists(), "wasm still missing after build: {wasm:?}");
    }
    wasm
}

fn read_counter() -> u64 {
    let (raw, _cas) = shared_data_get(b"on_tick.count").expect("counter present");
    std::str::from_utf8(&raw).ok().and_then(|s| s.parse::<u64>().ok()).unwrap_or(0)
}

// ─────────────────────────────────────────────────────────
// 主测试
// ─────────────────────────────────────────────────────────

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn proxy_on_tick_drives_background_ticks() {
    // 把 shared_data 计数器清零（cas=0 = 无 CAS 期望）。
    let _ = shared_data_set(b"on_tick.count", b"0", 0);

    let wasm = ensure_guest_built();
    let plugin_config = PluginConfig {
        id: PluginInstanceId {
            code: "wasm".into(),
            name: PluginInstanceName::named("on-tick-test"),
        },
        spec: serde_json::json!({
            "url": format!("file://{}", wasm.display()),
            "plugin_name": "on-tick-plugin",
            "plugin_root_id": "on-tick-root",
            "plugin_vm_id": "default",
        }),
    };
    let shell = WasmPluginShell::create(plugin_config).expect("Plugin::create");

    // shell 内部已经 spawn 了 50ms 颗粒度的 tick 任务；
    // 期间 guest `on_vm_start` 把 period 设成 50ms。
    // 等 450ms 至少 4 次 tick（保留 CI / 本地调度抖动余量）。
    tokio::time::sleep(Duration::from_millis(450)).await;

    let count = read_counter();
    assert!(count >= 4, "expected >= 4 ticks in 450ms, got {count}");
    tracing::info!("got {count} ticks");

    // 取一次 snapshot，drop 之后再 sleep 同等时间，断言不再继续增长（允许 1 次余量：
    // task abort 与正在执行中的同步 tick() 之间可能交叠一次）。
    let snapshot = count;
    drop(shell);
    tokio::time::sleep(Duration::from_millis(200)).await;
    let after = read_counter();
    assert!(
        after.saturating_sub(snapshot) <= 1,
        "tick task should stop after shell drop: snapshot={snapshot}, after={after}",
    );
}
