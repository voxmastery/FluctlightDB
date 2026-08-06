//! WAL fsync policy — acknowledged writes are durable by default.

use std::fs::File;
use std::path::Path;
use std::time::Duration;

use crate::error::{Error, Result};

pub fn wal_fsync_mode() -> WalFsyncMode {
    let configured = std::env::var("FLUCTLIGHT_WAL_FSYNC").unwrap_or_else(|_| "always".into());
    wal_fsync_mode_for(&configured)
}

fn wal_fsync_mode_for(configured: &str) -> WalFsyncMode {
    match configured.to_lowercase().as_str() {
        "none" | "never" => WalFsyncMode::None,
        // Historical "batched" mode could acknowledge before fsync; treat as Always.
        "always" | "strict" | "batched" | "batch" => WalFsyncMode::Always,
        _ => WalFsyncMode::Always,
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WalFsyncMode {
    Always,
    None,
}

fn inject_fsync_fault() -> Result<()> {
    let enabled = std::env::var("FLUCTLIGHT_ENABLE_FAULT_INJECTION")
        .map(|value| value == "1")
        .unwrap_or(false);
    if !enabled {
        return Ok(());
    }
    if std::env::var("FLUCTLIGHT_FAULT_DISK_FULL").as_deref() == Ok("1") {
        return Err(Error::Io(std::io::Error::new(
            std::io::ErrorKind::StorageFull,
            "injected disk full before durable acknowledgement",
        )));
    }
    if let Ok(delay_ms) = std::env::var("FLUCTLIGHT_FAULT_FSYNC_DELAY_MS") {
        let delay_ms = delay_ms
            .parse::<u64>()
            .map_err(|_| Error::Store("invalid injected fsync delay".into()))?;
        std::thread::sleep(Duration::from_millis(delay_ms.min(60_000)));
    }
    Ok(())
}

pub fn append_and_sync(_brain_path: &Path, file: &mut File, _line_bytes: usize) -> Result<()> {
    inject_fsync_fault()?;
    match wal_fsync_mode() {
        WalFsyncMode::None => Ok(()),
        WalFsyncMode::Always => file.sync_all().map_err(Error::Io),
    }
}

pub fn flush_path(_brain_path: &Path, file: &mut File) -> Result<()> {
    inject_fsync_fault()?;
    file.sync_all().map_err(Error::Io)
}

pub fn flush_all() -> Result<()> {
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_env::EnvGuard;
    use std::io::Write;
    use tempfile::tempdir;

    #[test]
    fn wal_fsync_defaults_to_always() {
        let previous = std::env::var_os("FLUCTLIGHT_WAL_FSYNC");
        std::env::remove_var("FLUCTLIGHT_WAL_FSYNC");
        assert_eq!(wal_fsync_mode(), WalFsyncMode::Always);
        match previous {
            Some(value) => std::env::set_var("FLUCTLIGHT_WAL_FSYNC", value),
            None => std::env::remove_var("FLUCTLIGHT_WAL_FSYNC"),
        }
    }

    #[test]
    fn explicit_batched_mode_is_a_durable_alias_for_always() {
        assert_eq!(wal_fsync_mode_for("batched"), WalFsyncMode::Always);
    }

    #[test]
    fn injected_disk_full_prevents_durable_ack() {
        let env = EnvGuard::acquire(&[
            "FLUCTLIGHT_ENABLE_FAULT_INJECTION",
            "FLUCTLIGHT_FAULT_DISK_FULL",
        ]);
        env.set("FLUCTLIGHT_ENABLE_FAULT_INJECTION", "1");
        env.set("FLUCTLIGHT_FAULT_DISK_FULL", "1");
        let dir = tempdir().unwrap();
        let path = dir.path().join("wal");
        let mut file = File::create(&path).unwrap();
        file.write_all(b"record").unwrap();

        let error = append_and_sync(&path, &mut file, 6).unwrap_err();
        assert!(error.to_string().contains("disk full"), "{error}");
    }

    #[test]
    fn injected_fsync_delay_is_observable_and_bounded() {
        let env = EnvGuard::acquire(&[
            "FLUCTLIGHT_ENABLE_FAULT_INJECTION",
            "FLUCTLIGHT_FAULT_FSYNC_DELAY_MS",
        ]);
        env.set("FLUCTLIGHT_ENABLE_FAULT_INJECTION", "1");
        env.set("FLUCTLIGHT_FAULT_FSYNC_DELAY_MS", "20");
        let dir = tempdir().unwrap();
        let path = dir.path().join("wal");
        let mut file = File::create(&path).unwrap();
        let started = std::time::Instant::now();
        append_and_sync(&path, &mut file, 0).unwrap();
        assert!(started.elapsed() >= Duration::from_millis(20));
        assert!(started.elapsed() < Duration::from_secs(2));
    }
}
