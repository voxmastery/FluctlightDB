//! Per-tenant request rate limiting (token bucket).

use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};
use std::time::Instant;

const DEFAULT_CAPACITY: u32 = 120;
const DEFAULT_REFILL_PER_SEC: u32 = 60;

struct Bucket {
    tokens: f32,
    last: Instant,
    capacity: f32,
    refill: f32,
}

impl Bucket {
    fn new(capacity: u32, refill_per_sec: u32) -> Self {
        Self {
            tokens: capacity as f32,
            last: Instant::now(),
            capacity: capacity as f32,
            refill: refill_per_sec as f32,
        }
    }

    fn allow(&mut self) -> bool {
        let now = Instant::now();
        let elapsed = now.duration_since(self.last).as_secs_f32();
        self.tokens = (self.tokens + elapsed * self.refill).min(self.capacity);
        self.last = now;
        if self.tokens >= 1.0 {
            self.tokens -= 1.0;
            true
        } else {
            false
        }
    }
}

static BUCKETS: OnceLock<Mutex<HashMap<String, Bucket>>> = OnceLock::new();

fn buckets() -> &'static Mutex<HashMap<String, Bucket>> {
    BUCKETS.get_or_init(|| Mutex::new(HashMap::new()))
}

pub fn allow(tenant_id: &str) -> bool {
    let capacity = std::env::var("FLUCTLIGHT_RATE_CAPACITY")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(DEFAULT_CAPACITY);
    let refill = std::env::var("FLUCTLIGHT_RATE_REFILL")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(DEFAULT_REFILL_PER_SEC);
    let mut guard = buckets().lock().unwrap();
    let bucket = guard
        .entry(tenant_id.to_string())
        .or_insert_with(|| Bucket::new(capacity, refill));
    bucket.allow()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bucket_allows_burst() {
        let mut b = Bucket::new(5, 1);
        for _ in 0..5 {
            assert!(b.allow());
        }
        assert!(!b.allow());
    }
}
