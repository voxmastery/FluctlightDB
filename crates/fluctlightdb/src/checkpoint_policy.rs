//! Batched checkpoint policy — WAL-first durability, amortized snapshot I/O.

use std::time::{Duration, Instant};

#[derive(Debug, Clone)]
pub struct CheckpointPolicy {
    pub every_n_writes: u64,
    pub interval: Duration,
    writes_since: u64,
    last_checkpoint: Instant,
}

impl Default for CheckpointPolicy {
    fn default() -> Self {
        Self::from_env()
    }
}

impl CheckpointPolicy {
    pub fn from_env() -> Self {
        let every_n_writes = std::env::var("FLUCTLIGHT_CHECKPOINT_EVERY_N")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(1);
        let interval_secs = std::env::var("FLUCTLIGHT_CHECKPOINT_INTERVAL_SEC")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(0);
        Self {
            every_n_writes: every_n_writes.max(1),
            interval: Duration::from_secs(interval_secs),
            writes_since: 0,
            last_checkpoint: Instant::now(),
        }
    }

    pub fn note_write(&mut self) {
        self.writes_since += 1;
    }

    pub fn should_checkpoint(&mut self) -> bool {
        if self.every_n_writes <= 1 && self.interval.is_zero() {
            return true;
        }
        let by_count = self.writes_since >= self.every_n_writes;
        let by_time = !self.interval.is_zero() && self.last_checkpoint.elapsed() >= self.interval;
        by_count || by_time
    }

    pub fn mark_checkpointed(&mut self) {
        self.writes_since = 0;
        self.last_checkpoint = Instant::now();
    }
}
