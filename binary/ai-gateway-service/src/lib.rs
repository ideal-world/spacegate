//! AI Gateway Service library — 供集成测试与二进制共用。
pub mod app;

#[cfg(feature = "test-support")]
pub use app::test_support::{CallbackRecord, HarnessConfig, TestHarness};
