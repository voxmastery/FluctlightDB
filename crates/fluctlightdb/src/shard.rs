//! Tenant → shard routing for horizontal scale-out.

use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};

#[derive(Debug, Clone)]
pub struct ShardRouter {
    pub shard_count: u32,
    pub base_port: u16,
    pub host: String,
}

impl Default for ShardRouter {
    fn default() -> Self {
        Self::from_env()
    }
}

impl ShardRouter {
    pub fn from_env() -> Self {
        Self {
            shard_count: std::env::var("FLUCTLIGHT_SHARD_COUNT")
                .ok()
                .and_then(|v| v.parse().ok())
                .unwrap_or(1)
                .max(1),
            base_port: std::env::var("FLUCTLIGHT_SHARD_BASE_PORT")
                .ok()
                .and_then(|v| v.parse().ok())
                .unwrap_or(8792),
            host: std::env::var("FLUCTLIGHT_SHARD_HOST").unwrap_or_else(|_| "127.0.0.1".into()),
        }
    }

    pub fn shard_for(&self, tenant_id: &str) -> u32 {
        if self.shard_count <= 1 {
            return 0;
        }
        let mut h = DefaultHasher::new();
        tenant_id.hash(&mut h);
        (h.finish() as u32) % self.shard_count
    }

    pub fn serve_addr(&self, tenant_id: &str) -> String {
        let shard = self.shard_for(tenant_id);
        format!("{}:{}", self.host, self.base_port + shard as u16)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stable_shard_assignment() {
        let r = ShardRouter {
            shard_count: 8,
            base_port: 8792,
            host: "127.0.0.1".into(),
        };
        assert_eq!(r.shard_for("user_a"), r.shard_for("user_a"));
        assert_ne!(r.shard_for("user_a"), r.shard_for("user_b"));
    }
}
