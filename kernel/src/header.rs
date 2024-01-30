#![allow(clippy::declare_interior_mutable_const)]
use hyper::header::HeaderName;

pub const X_FORWARDED_FOR: HeaderName = HeaderName::from_static("x-forwarded-for");
