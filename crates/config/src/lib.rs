#![warn(clippy::indexing_slicing, clippy::unwrap_used, clippy::dbg_macro, clippy::undocumented_unsafe_blocks)]
//! This crate is aim to manage the configuration of spacegate application with various backends.

/// re-export spacegate_model
pub mod model;
/// Configuration backends and services traits
pub mod service;

pub use model::*;
