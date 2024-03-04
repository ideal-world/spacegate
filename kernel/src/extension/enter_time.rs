use std::time::{Duration, Instant};
/// the time when request enter service
#[derive(Debug, Clone, Copy)]
pub struct EnterTime(pub Instant);
impl EnterTime {
    pub fn new() -> Self {
        Self(Instant::now())
    }
    pub fn elapsed(&self) -> Duration {
        self.0.elapsed()
    }
}

impl Default for EnterTime {
    fn default() -> Self {
        Self::new()
    }
}
