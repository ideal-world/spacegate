use std::{borrow::Cow, future::Future};

use crate::{Config, ConfigItem};

pub mod fs;
pub mod k8s;
pub mod redis;