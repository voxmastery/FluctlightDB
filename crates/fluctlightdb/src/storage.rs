//! Storage layout resolution — v3 monolithic `.flct` vs v4 segmented directory.

use std::path::{Path, PathBuf};

use crate::tenant::default_tenant_root;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StorageFormat {
    V3File,
    V4Dir,
}

pub fn format_from_env() -> StorageFormat {
    match std::env::var("FLUCTLIGHT_STORAGE")
        .unwrap_or_else(|_| "v4".into())
        .to_lowercase()
        .as_str()
    {
        "v3" | "file" | "flct" => StorageFormat::V3File,
        _ => StorageFormat::V4Dir,
    }
}

pub fn is_v4_path(path: &Path) -> bool {
    path.is_dir() || path.join("manifest.json").exists()
}

pub fn default_brain_path() -> PathBuf {
    match format_from_env() {
        StorageFormat::V3File => default_tenant_root().join("serverbrain.flct"),
        StorageFormat::V4Dir => default_tenant_brain_dir("default"),
    }
}

pub fn default_tenant_brain_dir(tenant_id: &str) -> PathBuf {
    default_tenant_root()
        .join("tenants")
        .join(tenant_id)
        .join("brain")
}

pub fn should_use_v4(path: &Path) -> bool {
    if is_v4_path(path) {
        return true;
    }
    if path.extension().and_then(|s| s.to_str()) == Some("flct") {
        return false;
    }
    format_from_env() == StorageFormat::V4Dir
}

pub fn lock_path(brain_path: &Path) -> PathBuf {
    if is_v4_path(brain_path) {
        brain_path.join(".brain.lock")
    } else {
        brain_path.with_extension("flct.lock")
    }
}

pub fn snapshot_path(brain_path: &Path) -> PathBuf {
    if is_v4_path(brain_path) {
        brain_path.join("manifest.json")
    } else {
        brain_path.to_path_buf()
    }
}
