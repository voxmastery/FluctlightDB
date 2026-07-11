//! Process-wide lock for tests that mutate `FLUCTLIGHT_*` environment variables.
//!
//! Cargo runs unit tests in parallel by default; unsynchronized `set_var` /
//! `remove_var` races are the usual cause of flaky CI (`cargo` exit 101).

use std::collections::HashMap;
use std::sync::{Mutex, MutexGuard};

static ENV_LOCK: Mutex<()> = Mutex::new(());

/// Hold this guard for the entire duration of any env mutation + assertions.
pub fn lock() -> MutexGuard<'static, ()> {
    ENV_LOCK
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

/// Snapshot current values of `keys`, then restore them when the guard drops.
pub struct EnvGuard {
    _lock: MutexGuard<'static, ()>,
    prior: HashMap<String, Option<String>>,
}

impl EnvGuard {
    pub fn acquire(keys: &[&str]) -> Self {
        let _lock = lock();
        let prior = keys
            .iter()
            .map(|k| ((*k).to_string(), std::env::var(k).ok()))
            .collect();
        Self { _lock, prior }
    }

    pub fn set(&self, key: &str, val: &str) {
        std::env::set_var(key, val);
    }

    pub fn remove(&self, key: &str) {
        std::env::remove_var(key);
    }
}

impl Drop for EnvGuard {
    fn drop(&mut self) {
        for (k, v) in &self.prior {
            match v {
                Some(val) => std::env::set_var(k, val),
                None => std::env::remove_var(k),
            }
        }
    }
}
