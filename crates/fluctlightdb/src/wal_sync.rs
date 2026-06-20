//! Batched WAL fsync — group durability flushes for write throughput.

use std::collections::HashMap;
use std::fs::File;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};

use crate::error::{Error, Result};

static WAL_SYNC_STATE: OnceLock<Mutex<HashMap<PathBuf, WalSyncSlot>>> = OnceLock::new();

struct WalSyncSlot {
    pending_bytes: u64,
    pending_records: u64,
    last_sync: Instant,
}

fn state_map() -> &'static Mutex<HashMap<PathBuf, WalSyncSlot>> {
    WAL_SYNC_STATE.get_or_init(|| Mutex::new(HashMap::new()))
}

pub fn wal_fsync_mode() -> WalFsyncMode {
    match std::env::var("FLUCTLIGHT_WAL_FSYNC")
        .unwrap_or_else(|_| "batched".into())
        .to_lowercase()
        .as_str()
    {
        "always" | "strict" => WalFsyncMode::Always,
        "none" | "never" => WalFsyncMode::None,
        _ => WalFsyncMode::Batched,
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WalFsyncMode {
    Always,
    Batched,
    None,
}

fn batch_bytes_limit() -> u64 {
    std::env::var("FLUCTLIGHT_WAL_BATCH_BYTES")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(256 * 1024)
}

fn batch_records_limit() -> u64 {
    std::env::var("FLUCTLIGHT_WAL_BATCH_RECORDS")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(32)
}

fn batch_interval() -> Duration {
    let ms = std::env::var("FLUCTLIGHT_WAL_BATCH_MS")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(25);
    Duration::from_millis(ms)
}

pub fn append_and_sync(brain_path: &Path, file: &mut File, line_bytes: usize) -> Result<()> {
    match wal_fsync_mode() {
        WalFsyncMode::None => Ok(()),
        WalFsyncMode::Always => file.sync_all().map_err(Error::Io),
        WalFsyncMode::Batched => {
            let key = brain_path.to_path_buf();
            let mut map = state_map().lock().map_err(|_| Error::Store("wal sync lock".into()))?;
            let slot = map.entry(key).or_insert(WalSyncSlot {
                pending_bytes: 0,
                pending_records: 0,
                last_sync: Instant::now(),
            });
            slot.pending_bytes += line_bytes as u64;
            slot.pending_records += 1;
            let due = slot.pending_bytes >= batch_bytes_limit()
                || slot.pending_records >= batch_records_limit()
                || slot.last_sync.elapsed() >= batch_interval();
            if due {
                file.sync_all().map_err(Error::Io)?;
                slot.pending_bytes = 0;
                slot.pending_records = 0;
                slot.last_sync = Instant::now();
            }
            Ok(())
        }
    }
}

pub fn flush_path(brain_path: &Path, file: &mut File) -> Result<()> {
    file.sync_all().map_err(Error::Io)?;
    if let Ok(mut map) = state_map().lock() {
        if let Some(slot) = map.get_mut(&brain_path.to_path_buf()) {
            slot.pending_bytes = 0;
            slot.pending_records = 0;
            slot.last_sync = Instant::now();
        }
    }
    Ok(())
}

pub fn flush_all() -> Result<()> {
    // Best-effort — callers checkpoint with explicit file handles.
    Ok(())
}
