//! Incremental replication state + delta sync.

use std::collections::HashMap;
use std::fs;
use std::io::{Read, Seek, Write};
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

use crate::brain::FluctlightBrain;
use crate::error::{Error, Result};
use crate::storage;
use crate::store;
use crate::wal;

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct SyncState {
    pub manifest_mtime: u64,
    pub wal_segments: HashMap<String, u64>,
    pub last_wal_seq: u64,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct ReplicaStatus {
    pub primary_path: String,
    pub replica_dir: String,
    pub snapshot_copied: bool,
    pub wal_segments: usize,
    pub wal_bytes_copied: u64,
    pub incremental: bool,
    pub lag_seconds: f64,
}

pub fn sync_once(primary: &Path, replica_dir: &Path) -> Result<ReplicaStatus> {
    fs::create_dir_all(replica_dir)?;
    let state_path = replica_dir.join("sync_state.json");
    let mut state = load_state(&state_path);
    let mut snapshot_copied = false;
    let mut wal_bytes_copied = 0u64;

    let manifest = primary.join("manifest.json");
    let manifest_mtime = file_mtime_secs(&manifest).unwrap_or(0);
    let brain_dst = replica_dir.join("brain");

    if storage::is_v4_path(primary) {
        if !brain_dst.exists() || state.manifest_mtime != manifest_mtime {
            incremental_copy_tree(primary, &brain_dst)?;
            snapshot_copied = true;
            state.manifest_mtime = manifest_mtime;
        }
    } else if primary.is_file() {
        let dst = replica_dir.join("brain.flct");
        let src_mtime = file_mtime_secs(primary).unwrap_or(0);
        if !dst.exists() || state.manifest_mtime != src_mtime {
            fs::copy(primary, &dst)?;
            snapshot_copied = true;
            state.manifest_mtime = src_mtime;
        }
    }

    let wal_dst_dir = if storage::is_v4_path(primary) {
        brain_dst.join("wal")
    } else {
        replica_dir.join("wal")
    };
    fs::create_dir_all(&wal_dst_dir)?;

    let mut wal_segments = 0usize;
    for seg in wal::list_segments_public(primary) {
        let name = seg
            .file_name()
            .and_then(|s| s.to_str())
            .unwrap_or("seg")
            .to_string();
        let src_len = fs::metadata(&seg).map(|m| m.len()).unwrap_or(0);
        let dst_path = wal_dst_dir.join(&name);
        let prev_len = state.wal_segments.get(&name).copied().unwrap_or(0);
        if src_len != prev_len || !dst_path.exists() {
            if prev_len > 0 && src_len > prev_len && dst_path.exists() {
                wal_bytes_copied += tail_copy(&seg, &dst_path, prev_len)?;
            } else {
                fs::copy(&seg, &dst_path)?;
                wal_bytes_copied += src_len;
            }
            state.wal_segments.insert(name, src_len);
        }
        wal_segments += 1;
    }

    save_state(&state_path, &state)?;

    let lag_seconds = fs::metadata(primary)
        .ok()
        .and_then(|m| m.modified().ok())
        .and_then(|t| SystemTime::now().duration_since(t).ok())
        .map(|d| d.as_secs_f64())
        .unwrap_or(0.0);

    Ok(ReplicaStatus {
        primary_path: primary.display().to_string(),
        replica_dir: replica_dir.display().to_string(),
        snapshot_copied,
        wal_segments,
        wal_bytes_copied,
        incremental: !snapshot_copied,
        lag_seconds,
    })
}

pub fn open_replica_brain(replica_dir: &Path) -> Result<FluctlightBrain> {
    let brain_path = if replica_dir.join("brain").join("manifest.json").exists() {
        replica_dir.join("brain")
    } else {
        replica_dir.join("brain.flct")
    };
    store::load(&brain_path)
}

pub fn run_tail_loop(primary: &Path, replica_dir: &Path, interval: Duration) -> Result<()> {
    loop {
        let status = sync_once(primary, replica_dir)?;
        eprintln!(
            "replica sync: incremental={} wal_segments={} wal_bytes={} lag_s={:.1}",
            status.incremental, status.wal_segments, status.wal_bytes_copied, status.lag_seconds
        );
        std::thread::sleep(interval);
    }
}

fn load_state(path: &Path) -> SyncState {
    if !path.exists() {
        return SyncState::default();
    }
    fs::read_to_string(path)
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default()
}

fn save_state(path: &Path, state: &SyncState) -> Result<()> {
    let json = serde_json::to_string_pretty(state).map_err(|e| Error::Serde(e.to_string()))?;
    fs::write(path, json).map_err(Error::Io)?;
    Ok(())
}

fn file_mtime_secs(path: &Path) -> Option<u64> {
    fs::metadata(path)
        .ok()
        .and_then(|m| m.modified().ok())
        .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
        .map(|d| d.as_secs())
}

fn incremental_copy_tree(src: &Path, dst: &Path) -> Result<()> {
    fs::create_dir_all(dst)?;
    for entry in fs::read_dir(src).map_err(Error::Io)? {
        let entry = entry.map_err(Error::Io)?;
        let from = entry.path();
        let to = dst.join(entry.file_name());
        if entry.file_type().map_err(Error::Io)?.is_dir() {
            incremental_copy_tree(&from, &to)?;
        } else {
            let copy = !to.exists()
                || fs::metadata(&from)
                    .and_then(|a| fs::metadata(&to).map(|b| (a, b)))
                    .map(|(a, b)| {
                        a.len() != b.len()
                            || a.modified().unwrap_or(UNIX_EPOCH)
                                != b.modified().unwrap_or(UNIX_EPOCH)
                    })
                    .unwrap_or(true);
            if copy {
                fs::copy(&from, &to).map_err(Error::Io)?;
            }
        }
    }
    Ok(())
}

fn tail_copy(src: &Path, dst: &Path, offset: u64) -> Result<u64> {
    let mut src_f = fs::File::open(src).map_err(Error::Io)?;
    let mut dst_f = fs::OpenOptions::new()
        .write(true)
        .open(dst)
        .map_err(Error::Io)?;
    src_f
        .seek(std::io::SeekFrom::Start(offset))
        .map_err(Error::Io)?;
    let mut buf = [0u8; 8192];
    let mut copied = 0u64;
    loop {
        let n = src_f.read(&mut buf).map_err(Error::Io)?;
        if n == 0 {
            break;
        }
        dst_f.write_all(&buf[..n]).map_err(Error::Io)?;
        copied += n as u64;
    }
    dst_f.sync_all().map_err(Error::Io)?;
    Ok(copied)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::manifest::save_v4_dir;
    use crate::{Episode, FluctlightBrain};
    use tempfile::tempdir;

    #[test]
    fn incremental_sync_skips_unchanged_manifest() {
        let dir = tempdir().unwrap();
        let primary = dir.path().join("primary");
        let replica = dir.path().join("replica");
        let mut brain = FluctlightBrain::new();
        brain
            .experience(Episode {
                content: "incremental".into(),
                context: "t".into(),
                outcome: None,
                salience_hint: 0.5,
                semantic_vector: None,
                agent_id: None,
                tenant_id: None,
                rag: None,
                provenance: None,
            })
            .unwrap();
        save_v4_dir(&brain, &primary).unwrap();
        let s1 = sync_once(&primary, &replica).unwrap();
        assert!(s1.snapshot_copied);
        let s2 = sync_once(&primary, &replica).unwrap();
        assert!(!s2.snapshot_copied);
    }
}
