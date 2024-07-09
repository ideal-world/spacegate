#[cfg(feature = "decompression")]
pub mod decompression;
#[cfg(feature = "header-modifier")]
pub mod header_modifier;
#[cfg(feature = "inject")]
pub mod inject;
#[cfg(feature = "limit")]
pub mod limit;
#[cfg(feature = "maintenance")]
pub mod maintenance;
#[cfg(feature = "redirect")]
pub mod redirect;
// #[cfg(feature = "retry")]
// pub mod retry;
#[cfg(feature = "rewrite")]
pub mod rewrite;
// #[cfg(feature = "status")]
// pub mod status;

#[cfg(feature = "set-version")]
pub mod set_version;
#[cfg(feature = "set-scheme")]
pub mod set_scheme;
pub mod static_resource;
