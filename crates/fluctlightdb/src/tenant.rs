//! Per-tenant brain configuration and storage layout.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TenantConfig {
    pub tenant_id: String,
    pub brain_path: PathBuf,
    pub max_synapses: usize,
    pub max_engrams: usize,
}

#[derive(Debug, Clone, Deserialize)]
struct TenantConfigFile {
    max_engrams: Option<usize>,
    max_synapses: Option<usize>,
}

fn env_usize(key: &str, default: usize) -> usize {
    std::env::var(key)
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(default)
}

impl TenantConfig {
    pub fn default_for(tenant_id: &str, base: &Path) -> Self {
        let root = base.join("tenants").join(tenant_id);
        let brain_path =
            if crate::storage::format_from_env() == crate::storage::StorageFormat::V4Dir {
                root.join("brain")
            } else {
                root.join("brain.flct")
            };
        let mut cfg = Self {
            tenant_id: tenant_id.to_string(),
            brain_path,
            max_synapses: env_usize("FLUCTLIGHT_MAX_SYNAPSES", 500_000),
            max_engrams: env_usize("FLUCTLIGHT_MAX_ENGRAMS", 50_000),
        };
        cfg.merge_file_config();
        cfg
    }

    pub fn with_brain_path(tenant_id: &str, base: &Path, brain_path: PathBuf) -> Self {
        let mut cfg = Self::default_for(tenant_id, base);
        cfg.brain_path = brain_path;
        cfg.merge_file_config();
        cfg
    }

    pub fn merge_file_config(&mut self) {
        let path = self.brain_path.parent().map(|p| p.join("config.json"));
        let Some(path) = path else {
            return;
        };
        if !path.exists() {
            return;
        }
        if let Ok(raw) = std::fs::read_to_string(&path) {
            if let Ok(file_cfg) = serde_json::from_str::<TenantConfigFile>(&raw) {
                if let Some(v) = file_cfg.max_engrams {
                    self.max_engrams = v;
                }
                if let Some(v) = file_cfg.max_synapses {
                    self.max_synapses = v;
                }
            }
        }
    }

    pub fn ensure_dirs(&self) -> std::io::Result<()> {
        if let Some(parent) = self.brain_path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        Ok(())
    }

    pub fn check_limits(&self, brain: &crate::brain::FluctlightBrain) -> crate::error::Result<()> {
        if brain.hippocampus.engrams.len() >= self.max_engrams {
            return Err(crate::error::Error::Store(format!(
                "tenant {} engram limit {} exceeded",
                self.tenant_id, self.max_engrams
            )));
        }
        if brain.graph.synapse_count() >= self.max_synapses {
            return Err(crate::error::Error::Store(format!(
                "tenant {} synapse limit {} exceeded",
                self.tenant_id, self.max_synapses
            )));
        }
        Ok(())
    }
}

pub fn default_tenant_root() -> PathBuf {
    std::env::var("HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from("."))
        .join(".fluctlight")
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn tenant_layout() {
        let dir = tempdir().unwrap();
        let cfg = TenantConfig::default_for("agent_a", dir.path());
        assert!(
            cfg.brain_path.ends_with("tenants/agent_a/brain")
                || cfg.brain_path.ends_with("tenants/agent_a/brain.flct")
        );
    }

    #[test]
    fn tenant_file_config_overrides_limits() {
        let dir = tempdir().unwrap();
        let tenant_root = dir.path().join("tenants").join("tier_a");
        std::fs::create_dir_all(&tenant_root).unwrap();
        std::fs::write(
            tenant_root.join("config.json"),
            r#"{"max_engrams": 42, "max_synapses": 1000}"#,
        )
        .unwrap();
        let cfg = TenantConfig::default_for("tier_a", dir.path());
        assert_eq!(cfg.max_engrams, 42);
        assert_eq!(cfg.max_synapses, 1000);
    }
}
