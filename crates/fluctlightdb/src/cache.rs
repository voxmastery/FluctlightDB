//! LRU activation cache — repeat cue recalls skip graph work.

use std::collections::HashMap;
use std::hash::{Hash, Hasher};
use std::time::{Duration, Instant};

use crate::types::ActivationResult;

const DEFAULT_MAX_ENTRIES: usize = 512;
const DEFAULT_TTL: Duration = Duration::from_secs(60);

#[derive(Debug, Clone)]
struct CacheEntry {
    result: ActivationResult,
    inserted: Instant,
}

#[derive(Debug, Clone, Default)]
pub struct ActivationCache {
    entries: HashMap<u64, CacheEntry>,
    max_entries: usize,
    ttl: Duration,
}

impl ActivationCache {
    pub fn new() -> Self {
        Self {
            entries: HashMap::new(),
            max_entries: cache_max_entries(),
            ttl: cache_ttl(),
        }
    }

    pub fn get(&self, cue: &str, agent_id: Option<&str>) -> Option<ActivationResult> {
        if !cache_enabled() {
            return None;
        }
        let key = cache_key(cue, agent_id);
        self.entries.get(&key).and_then(|e| {
            if e.inserted.elapsed() < self.ttl {
                Some(e.result.clone())
            } else {
                None
            }
        })
    }

    pub fn put(&mut self, cue: &str, agent_id: Option<&str>, result: ActivationResult) {
        if !cache_enabled() {
            return;
        }
        if self.entries.len() >= self.max_entries {
            self.evict_expired();
        }
        if self.entries.len() >= self.max_entries {
            if let Some(k) = self.entries.keys().next().copied() {
                self.entries.remove(&k);
            }
        }
        let key = cache_key(cue, agent_id);
        self.entries.insert(
            key,
            CacheEntry {
                result,
                inserted: Instant::now(),
            },
        );
    }

    pub fn invalidate(&mut self) {
        self.entries.clear();
    }

    fn evict_expired(&mut self) {
        self.entries.retain(|_, e| e.inserted.elapsed() < self.ttl);
    }
}

fn cache_key(cue: &str, agent_id: Option<&str>) -> u64 {
    use std::collections::hash_map::DefaultHasher;
    let mut h = DefaultHasher::new();
    cue.hash(&mut h);
    agent_id.hash(&mut h);
    h.finish()
}

pub fn cache_enabled() -> bool {
    std::env::var("FLUCTLIGHT_ACTIVATE_CACHE")
        .map(|v| v != "0" && v.to_lowercase() != "false")
        .unwrap_or(true)
}

fn cache_max_entries() -> usize {
    std::env::var("FLUCTLIGHT_CACHE_MAX")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(DEFAULT_MAX_ENTRIES)
}

fn cache_ttl() -> Duration {
    let secs = std::env::var("FLUCTLIGHT_CACHE_TTL_SECS")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(DEFAULT_TTL.as_secs());
    Duration::from_secs(secs.max(1))
}
