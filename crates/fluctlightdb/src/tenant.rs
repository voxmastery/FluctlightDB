//! Per-tenant brain configuration and storage layout (CAB locus).

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

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

/// True when `tenant_id` is safe as a single path segment (legacy layout only).
pub fn is_safe_legacy_tenant_id(tenant_id: &str) -> bool {
    if tenant_id.is_empty() || tenant_id.len() > 128 {
        return false;
    }
    let mut chars = tenant_id.chars();
    let Some(first) = chars.next() else {
        return false;
    };
    if !(first.is_ascii_alphanumeric()) {
        return false;
    }
    tenant_id
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
        && !tenant_id.contains("..")
}

/// Stable path-safe slug: first 32 hex chars of SHA-256(tenant_id).
pub fn locus_slug(tenant_id: &str) -> String {
    let digest = Sha256::digest(tenant_id.as_bytes());
    let mut out = String::with_capacity(32);
    for b in digest.iter().take(16) {
        out.push_str(&format!("{b:02x}"));
    }
    out
}

/// Resolve tenant directory under `base/tenants` with CAB locus rules.
///
/// - Prefer hashed locus if it exists.
/// - Else prefer legacy `tenants/<id>` only when `id` is a safe single segment and exists.
/// - New tenants always use the hashed locus (never raw join of untrusted ids).
pub fn tenant_dir(base: &Path, tenant_id: &str) -> PathBuf {
    let tenants_root = base.join("tenants");
    let hashed = tenants_root.join(locus_slug(tenant_id));
    if hashed.exists() {
        return hashed;
    }
    if is_safe_legacy_tenant_id(tenant_id) {
        let legacy = tenants_root.join(tenant_id);
        if legacy.exists() {
            return legacy;
        }
    }
    hashed
}

/// Ensure resolved path stays under `base/tenants` (after canonicalize when possible).
pub fn assert_locus_contained(base: &Path, dir: &Path) -> Result<(), String> {
    let tenants_root = base.join("tenants");
    let root_exists = tenants_root.exists();
    let root = if root_exists {
        std::fs::canonicalize(&tenants_root).map_err(|e| e.to_string())?
    } else {
        tenants_root.clone()
    };
    let contained = if dir.exists() {
        std::fs::canonicalize(dir)
            .map_err(|e| e.to_string())?
            .starts_with(&root)
    } else if !root_exists {
        dir.parent() == Some(tenants_root.as_path())
    } else {
        dir.parent()
            .and_then(|p| std::fs::canonicalize(p).ok())
            .map(|p| p == root || p.starts_with(&root))
            .unwrap_or(false)
    };
    if !contained {
        return Err(format!(
            "tenant locus escapes tenants root: {} (root={})",
            dir.display(),
            root.display()
        ));
    }
    Ok(())
}

impl TenantConfig {
    pub fn try_default_for(tenant_id: &str, base: &Path) -> Result<Self, String> {
        let root = tenant_dir(base, tenant_id);
        assert_locus_contained(base, &root)?;
        Ok(Self::default_for_root(tenant_id, root))
    }

    pub fn default_for(tenant_id: &str, base: &Path) -> Self {
        let root = tenant_dir(base, tenant_id);
        Self::default_for_root(tenant_id, root)
    }

    fn default_for_root(tenant_id: &str, root: PathBuf) -> Self {
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
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                std::fs::set_permissions(parent, std::fs::Permissions::from_mode(0o700))?;
            }
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
    if let Ok(root) = std::env::var("FLUCTLIGHT_TENANT_ROOT") {
        if !root.trim().is_empty() {
            return PathBuf::from(root);
        }
    }
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
    fn tenant_layout_uses_hashed_locus_for_new() {
        let dir = tempdir().unwrap();
        let cfg = TenantConfig::default_for("agent_a", dir.path());
        let slug = locus_slug("agent_a");
        assert!(
            cfg.brain_path.ends_with(format!("tenants/{slug}/brain"))
                || cfg
                    .brain_path
                    .ends_with(format!("tenants/{slug}/brain.flct")),
            "path={}",
            cfg.brain_path.display()
        );
    }

    #[test]
    fn checked_tenant_config_supports_new_base_directory() {
        let dir = tempdir().unwrap();
        let base = dir.path().join("new-root");
        let cfg = TenantConfig::try_default_for("agent_a", &base).unwrap();
        assert!(cfg.brain_path.starts_with(base.join("tenants")));
    }

    #[test]
    fn production_paths_do_not_use_unchecked_tenant_config() {
        for (name, source) in [
            ("serve.rs", include_str!("serve.rs")),
            (
                "fluctlight-cli/src/main.rs",
                include_str!("../../fluctlight-cli/src/main.rs"),
            ),
        ] {
            assert!(
                !source.contains("TenantConfig::default_for"),
                "{name} still bypasses tenant containment errors"
            );
        }
    }

    #[test]
    fn unsafe_tenant_id_never_joins_raw_path_segment() {
        let dir = tempdir().unwrap();
        let evil = "../PWNED";
        let cfg = TenantConfig::default_for(evil, dir.path());
        let s = cfg.brain_path.to_string_lossy();
        assert!(!s.contains("PWNED"), "{s}");
        assert!(s.contains(&locus_slug(evil)), "{s}");
    }

    #[cfg(unix)]
    #[test]
    fn tenant_config_rejects_hashed_locus_symlink_escape() {
        use std::os::unix::fs::symlink;

        let base = tempdir().unwrap();
        let outside = tempdir().unwrap();
        let tenants = base.path().join("tenants");
        std::fs::create_dir_all(&tenants).unwrap();
        symlink(outside.path(), tenants.join(locus_slug("escaped-tenant"))).unwrap();

        let err = TenantConfig::try_default_for("escaped-tenant", base.path()).unwrap_err();
        assert!(err.contains("escapes tenants root"), "{err}");
    }

    #[test]
    fn legacy_safe_id_still_resolves_existing_dir() {
        let dir = tempdir().unwrap();
        let legacy = dir.path().join("tenants").join("tier_a");
        std::fs::create_dir_all(&legacy).unwrap();
        std::fs::write(
            legacy.join("config.json"),
            r#"{"max_engrams": 42, "max_synapses": 1000}"#,
        )
        .unwrap();
        let cfg = TenantConfig::default_for("tier_a", dir.path());
        assert_eq!(cfg.max_engrams, 42);
        assert_eq!(cfg.max_synapses, 1000);
        assert!(
            cfg.brain_path.starts_with(&legacy)
                || cfg.brain_path.parent() == Some(legacy.as_path())
        );
    }

    #[test]
    fn safe_legacy_id_helper() {
        assert!(is_safe_legacy_tenant_id("agent_a"));
        assert!(!is_safe_legacy_tenant_id("../x"));
        assert!(!is_safe_legacy_tenant_id("a/b"));
        assert!(!is_safe_legacy_tenant_id("C:\\foo"));
    }

    #[cfg(unix)]
    #[test]
    fn tenant_directories_are_owner_only() {
        use std::os::unix::fs::PermissionsExt;

        let dir = tempdir().unwrap();
        let cfg = TenantConfig::try_default_for("private", dir.path()).unwrap();
        cfg.ensure_dirs().unwrap();
        let mode = std::fs::metadata(cfg.brain_path.parent().unwrap())
            .unwrap()
            .permissions()
            .mode()
            & 0o777;
        assert_eq!(mode, 0o700);
    }
}
