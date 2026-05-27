//! 进程级共享状态：spec §Shared Key-Value Store / §Shared Queues / §Metrics。
//!
//! 这些设施按 proxy-wasm spec 必须在多个 VM / plugin 实例之间共享，因此放在进程级 `OnceCell`
//! + `RwLock` 之后；不依赖具体 `HostState`。
//!
//! 实现要点：
//! - **Shared Data**：键值 + CAS（compare-and-swap）。每次成功 set 都使 cas 自增；
//!   guest 传 `cas=0` 表示不校验。
//! - **Shared Queues**：通过 `register_shared_queue(name)` / `resolve_shared_queue(vm_id, name)` 拿 qid；
//!   `enqueue`/`dequeue` 操作 `VecDeque<Vec<u8>>`。
//! - **Metrics**：Counter / Gauge / Histogram。Counter 不允许 decrement；Histogram 这里按 Gauge 处理
//!   （proxy-wasm 0.2.1 没有规定 histogram 的内部表示），足以满足 guest 的调用语义。

use std::collections::{HashMap, VecDeque};
use std::sync::{Mutex, RwLock};

use once_cell::sync::Lazy;
use opentelemetry::global;

use crate::abi::MetricType;

// ─────────────────────────────────────────────────────────
// Shared Data（spec §Shared Key-Value Store）
// ─────────────────────────────────────────────────────────

#[derive(Debug, Default, Clone)]
pub struct SharedDataEntry {
    pub value: Vec<u8>,
    pub cas: u32,
}

#[derive(Debug, Default)]
struct SharedDataStore {
    map: HashMap<Vec<u8>, SharedDataEntry>,
}

static SHARED_DATA: Lazy<RwLock<SharedDataStore>> = Lazy::new(|| RwLock::new(SharedDataStore::default()));

/// 读：返回 (value, cas)；不存在返回 `None`。
pub fn shared_data_get(key: &[u8]) -> Option<(Vec<u8>, u32)> {
    let g = SHARED_DATA.read().ok()?;
    g.map.get(key).map(|e| (e.value.clone(), e.cas))
}

#[derive(Debug, PartialEq, Eq)]
pub enum SharedDataSetResult {
    Ok,
    CasMismatch,
}

/// 写：cas==0 表示不校验；非 0 必须等于当前 cas 才能成功。
pub fn shared_data_set(key: &[u8], value: &[u8], cas: u32) -> SharedDataSetResult {
    let Ok(mut g) = SHARED_DATA.write() else {
        return SharedDataSetResult::CasMismatch;
    };
    let entry = g.map.entry(key.to_vec()).or_default();
    if cas != 0 && cas != entry.cas {
        return SharedDataSetResult::CasMismatch;
    }
    entry.value = value.to_vec();
    entry.cas = entry.cas.wrapping_add(1).max(1);
    SharedDataSetResult::Ok
}

// ─────────────────────────────────────────────────────────
// Shared Queues（spec §Shared Queues）
// ─────────────────────────────────────────────────────────

#[derive(Debug, Default)]
struct SharedQueueRegistry {
    by_id: HashMap<u32, VecDeque<Vec<u8>>>,
    by_name: HashMap<(String, String), u32>, // (vm_id, name) -> qid
    next_id: u32,
}

static SHARED_QUEUES: Lazy<Mutex<SharedQueueRegistry>> = Lazy::new(|| Mutex::new(SharedQueueRegistry::default()));

/// 注册（或打开已存在）一个共享队列；返回 qid。
///
/// `vm_id` 取本 VM 的 plugin_vm_id（按 spec 是 host 实现细节；这里用 "default"）。
pub fn queue_register(vm_id: &str, name: &str) -> u32 {
    let mut g = match SHARED_QUEUES.lock() {
        Ok(g) => g,
        Err(p) => p.into_inner(),
    };
    let key = (vm_id.to_string(), name.to_string());
    if let Some(qid) = g.by_name.get(&key).copied() {
        return qid;
    }
    g.next_id = g.next_id.wrapping_add(1).max(1);
    let qid = g.next_id;
    g.by_id.insert(qid, VecDeque::new());
    g.by_name.insert(key, qid);
    qid
}

/// 解析已存在的队列；不存在返回 None。
pub fn queue_resolve(vm_id: &str, name: &str) -> Option<u32> {
    let g = SHARED_QUEUES.lock().ok()?;
    g.by_name.get(&(vm_id.to_string(), name.to_string())).copied()
}

#[derive(Debug, PartialEq, Eq)]
pub enum QueueOpResult {
    Ok,
    NotFound,
    Empty,
}

pub fn queue_enqueue(qid: u32, value: &[u8]) -> QueueOpResult {
    let mut g = match SHARED_QUEUES.lock() {
        Ok(g) => g,
        Err(p) => p.into_inner(),
    };
    match g.by_id.get_mut(&qid) {
        Some(q) => {
            q.push_back(value.to_vec());
            QueueOpResult::Ok
        }
        None => QueueOpResult::NotFound,
    }
}

pub fn queue_dequeue(qid: u32) -> (QueueOpResult, Option<Vec<u8>>) {
    let mut g = match SHARED_QUEUES.lock() {
        Ok(g) => g,
        Err(p) => p.into_inner(),
    };
    match g.by_id.get_mut(&qid) {
        Some(q) => match q.pop_front() {
            Some(v) => (QueueOpResult::Ok, Some(v)),
            None => (QueueOpResult::Empty, None),
        },
        None => (QueueOpResult::NotFound, None),
    }
}

// ─────────────────────────────────────────────────────────
// Metrics（spec §Metrics）
// ─────────────────────────────────────────────────────────

#[derive(Debug)]
struct MetricEntry {
    kind: MetricType,
    value: u64,
    instrument: OtelMetricInstrument,
}

#[derive(Debug)]
enum OtelMetricInstrument {
    Counter(opentelemetry::metrics::Counter<u64>),
    Gauge(opentelemetry::metrics::Gauge<i64>),
    Histogram(opentelemetry::metrics::Histogram<u64>),
}

#[derive(Debug, Default)]
struct MetricRegistry {
    by_id: HashMap<u32, MetricEntry>,
    by_name: HashMap<String, u32>,
    next_id: u32,
}

static METRICS: Lazy<Mutex<MetricRegistry>> = Lazy::new(|| Mutex::new(MetricRegistry::default()));

#[derive(Debug, PartialEq, Eq)]
pub enum MetricOpResult {
    Ok,
    NotFound,
    BadArgument,
}

pub fn metric_define(kind: MetricType, name: &str) -> u32 {
    let mut g = match METRICS.lock() {
        Ok(g) => g,
        Err(p) => p.into_inner(),
    };
    if let Some(id) = g.by_name.get(name).copied() {
        return id;
    }
    g.next_id = g.next_id.wrapping_add(1).max(1);
    let id = g.next_id;
    let meter = global::meter("spacegate_plugin_wasm");
    let instrument = match kind {
        MetricType::Counter => OtelMetricInstrument::Counter(meter.u64_counter(name.to_string()).build()),
        MetricType::Gauge => OtelMetricInstrument::Gauge(meter.i64_gauge(name.to_string()).build()),
        MetricType::Histogram => OtelMetricInstrument::Histogram(meter.u64_histogram(name.to_string()).build()),
    };
    g.by_id.insert(id, MetricEntry { kind, value: 0, instrument });
    g.by_name.insert(name.to_string(), id);
    id
}

pub fn metric_record(id: u32, value: u64) -> MetricOpResult {
    let mut g = match METRICS.lock() {
        Ok(g) => g,
        Err(p) => p.into_inner(),
    };
    match g.by_id.get_mut(&id) {
        Some(m) => {
            m.value = value;
            match &m.instrument {
                OtelMetricInstrument::Counter(counter) => counter.add(value, &[]),
                OtelMetricInstrument::Gauge(gauge) => gauge.record(value as i64, &[]),
                OtelMetricInstrument::Histogram(histogram) => histogram.record(value, &[]),
            }
            MetricOpResult::Ok
        }
        None => MetricOpResult::NotFound,
    }
}

pub fn metric_increment(id: u32, delta: i64) -> MetricOpResult {
    let mut g = match METRICS.lock() {
        Ok(g) => g,
        Err(p) => p.into_inner(),
    };
    let Some(m) = g.by_id.get_mut(&id) else {
        return MetricOpResult::NotFound;
    };
    if matches!(m.kind, MetricType::Counter) && delta < 0 {
        return MetricOpResult::BadArgument;
    }
    if delta >= 0 {
        m.value = m.value.saturating_add(delta as u64);
    } else {
        m.value = m.value.saturating_sub((-delta) as u64);
    }
    match &m.instrument {
        OtelMetricInstrument::Counter(counter) => counter.add(delta.max(0) as u64, &[]),
        OtelMetricInstrument::Gauge(gauge) => gauge.record(m.value as i64, &[]),
        OtelMetricInstrument::Histogram(histogram) => histogram.record(m.value, &[]),
    }
    MetricOpResult::Ok
}

pub fn metric_get(id: u32) -> Option<u64> {
    let g = METRICS.lock().ok()?;
    g.by_id.get(&id).map(|m| m.value)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shared_data_cas_roundtrip() {
        let key = b"shared_data_cas_roundtrip_key";
        assert_eq!(shared_data_set(key, b"v1", 0), SharedDataSetResult::Ok);
        let (v, cas1) = shared_data_get(key).unwrap();
        assert_eq!(v, b"v1");
        assert!(cas1 > 0);
        assert_eq!(shared_data_set(key, b"v2", 99), SharedDataSetResult::CasMismatch);
        assert_eq!(shared_data_set(key, b"v2", cas1), SharedDataSetResult::Ok);
        let (v, cas2) = shared_data_get(key).unwrap();
        assert_eq!(v, b"v2");
        assert!(cas2 > cas1);
    }

    #[test]
    fn shared_queue_roundtrip() {
        let qid = queue_register("default", "shared_queue_roundtrip_q");
        assert_eq!(queue_enqueue(qid, b"a"), QueueOpResult::Ok);
        assert_eq!(queue_enqueue(qid, b"b"), QueueOpResult::Ok);
        let (s, v) = queue_dequeue(qid);
        assert_eq!(s, QueueOpResult::Ok);
        assert_eq!(v.as_deref(), Some(b"a".as_slice()));
        let (s, v) = queue_dequeue(qid);
        assert_eq!(s, QueueOpResult::Ok);
        assert_eq!(v.as_deref(), Some(b"b".as_slice()));
        let (s, _) = queue_dequeue(qid);
        assert_eq!(s, QueueOpResult::Empty);
    }

    #[test]
    fn metric_counter_increment_only() {
        let id = metric_define(MetricType::Counter, "metric_counter_increment_only");
        assert_eq!(metric_increment(id, 3), MetricOpResult::Ok);
        assert_eq!(metric_get(id), Some(3));
        assert_eq!(metric_increment(id, -1), MetricOpResult::BadArgument);
        assert_eq!(metric_get(id), Some(3));
    }

    #[test]
    fn metric_gauge_bidirectional() {
        let id = metric_define(MetricType::Gauge, "metric_gauge_bidirectional");
        assert_eq!(metric_increment(id, 5), MetricOpResult::Ok);
        assert_eq!(metric_increment(id, -2), MetricOpResult::Ok);
        assert_eq!(metric_get(id), Some(3));
        assert_eq!(metric_record(id, 100), MetricOpResult::Ok);
        assert_eq!(metric_get(id), Some(100));
    }
}
