//! Write-ahead log v2 — segmented rotation + checkpoint watermark replay.

use std::fs::{self, File, OpenOptions};
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::brain::FluctlightBrain;
use crate::error::{Error, Result};
use crate::sleep_trigger::SleepTrigger;
use crate::types::Episode;

const MAX_SEGMENT_BYTES: u64 = 64 * 1024 * 1024;

pub fn wal_enabled() -> bool {
    std::env::var("FLUCTLIGHT_WAL")
        .map(|v| v != "0" && !v.eq_ignore_ascii_case("false"))
        .unwrap_or(true)
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "op", rename_all = "snake_case")]
pub enum WalEntry {
    Experience { episode: Episode },
    Sleep,
    Tick { n: u64 },
    Reward { magnitude: f32 },
    MarkCore { engram_id: uuid::Uuid, key: String },
    Death { cause: String },
    Compact,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct WalRecord {
    seq: u64,
    #[serde(default)]
    idempotency_key: Option<String>,
    #[serde(flatten)]
    entry: WalEntry,
}

pub fn wal_base(brain_path: &Path) -> PathBuf {
    if crate::storage::is_v4_path(brain_path) {
        brain_path.join("wal").join("brain.wal")
    } else {
        brain_path.with_extension("flct.wal")
    }
}

pub fn wal_path(brain_path: &Path) -> PathBuf {
    active_segment(brain_path)
}

fn active_segment(brain_path: &Path) -> PathBuf {
    let base = wal_base(brain_path);
    let mut idx = 1u32;
    loop {
        let path = segment_path(&base, idx);
        if !path.exists() {
            return path;
        }
        let len = fs::metadata(&path).map(|m| m.len()).unwrap_or(0);
        if len < MAX_SEGMENT_BYTES {
            return path;
        }
        idx += 1;
    }
}

fn segment_path(base: &Path, idx: u32) -> PathBuf {
    PathBuf::from(format!("{}.{:03}", base.display(), idx))
}

pub fn list_segments(brain_path: &Path) -> Vec<PathBuf> {
    list_segments_inner(brain_path)
}

pub(crate) fn list_segments_public(brain_path: &Path) -> Vec<PathBuf> {
    list_segments_inner(brain_path)
}

fn list_segments_inner(brain_path: &Path) -> Vec<PathBuf> {
    let base = wal_base(brain_path);
    let parent = base.parent().unwrap_or(Path::new("."));
    let stem = base
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or("brain.flct.wal");
    let mut out = Vec::new();
    if let Ok(read) = fs::read_dir(parent) {
        for entry in read.flatten() {
            let name = entry.file_name();
            let name = name.to_string_lossy();
            if name.starts_with(stem) && name.contains('.') {
                out.push(entry.path());
            }
        }
    }
    out.sort();
    if out.is_empty() && base.exists() {
        out.push(base);
    }
    out
}

pub fn append(brain_path: &Path, seq: u64, entry: &WalEntry) -> Result<()> {
    let path = active_segment(brain_path);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let record = WalRecord {
        seq,
        idempotency_key: None,
        entry: entry.clone(),
    };
    let line = serde_json::to_string(&record).map_err(|e| Error::Serde(e.to_string()))?;
    let mut file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(&path)?;
    writeln!(file, "{line}")?;
    crate::wal_sync::append_and_sync(brain_path, &mut file, line.len() + 1)?;
    Ok(())
}

pub fn flush(brain_path: &Path) -> Result<()> {
    let path = active_segment(brain_path);
    if !path.exists() {
        return Ok(());
    }
    let mut file = OpenOptions::new().append(true).open(&path)?;
    crate::wal_sync::flush_path(brain_path, &mut file)
}

pub fn truncate(brain_path: &Path) -> Result<()> {
    for path in list_segments(brain_path) {
        if path.exists() {
            fs::remove_file(path)?;
        }
    }
    Ok(())
}

pub fn replay(brain: &mut FluctlightBrain, brain_path: &Path, after_seq: u64) -> Result<u64> {
    let mut count = 0u64;
    for path in list_segments(brain_path) {
        if !path.exists() {
            continue;
        }
        let file = File::open(&path)?;
        let reader = BufReader::new(file);
        for line in reader.lines() {
            let line = line?;
            if line.trim().is_empty() {
                continue;
            }
            let record: WalRecord =
                serde_json::from_str(&line).map_err(|e| Error::Serde(e.to_string()))?;
            if record.seq <= after_seq {
                continue;
            }
            apply_entry(brain, record.entry)?;
            count += 1;
        }
    }
    Ok(count)
}

pub fn replay_with_corruption_skip(
    brain: &mut FluctlightBrain,
    brain_path: &Path,
    after_seq: u64,
) -> Result<(u64, u64)> {
    let mut count = 0u64;
    let mut skipped = 0u64;
    for path in list_segments(brain_path) {
        if !path.exists() {
            continue;
        }
        let file = File::open(&path)?;
        let reader = BufReader::new(file);
        for line in reader.lines() {
            let line = match line {
                Ok(l) => l,
                Err(_) => {
                    skipped += 1;
                    continue;
                }
            };
            if line.trim().is_empty() {
                continue;
            }
            let record: WalRecord = match serde_json::from_str(&line) {
                Ok(r) => r,
                Err(_) => {
                    skipped += 1;
                    continue;
                }
            };
            if record.seq <= after_seq {
                continue;
            }
            apply_entry(brain, record.entry)?;
            count += 1;
        }
    }
    Ok((count, skipped))
}

fn apply_entry(brain: &mut FluctlightBrain, entry: WalEntry) -> Result<()> {
    match entry {
        WalEntry::Experience { episode } => {
            brain.experience_internal(episode, false)?;
        }
        WalEntry::Sleep => {
            brain.sleep_internal(false, SleepTrigger::Manual)?;
        }
        WalEntry::Tick { n } => {
            for _ in 0..n {
                brain.tick_internal(false)?;
            }
        }
        WalEntry::Reward { magnitude } => {
            brain.reward_internal(magnitude);
        }
        WalEntry::MarkCore { engram_id, key } => {
            brain.mark_core_internal(engram_id, key);
        }
        WalEntry::Death { cause } => {
            brain.death(&cause)?;
        }
        WalEntry::Compact => {
            brain.compact_internal(false)?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::Episode;
    use tempfile::tempdir;

    #[test]
    fn wal_replays_experience_after_checkpoint_gap() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("test.flct");
        let brain = FluctlightBrain::open(&path).unwrap();
        brain.checkpoint().unwrap();
        drop(brain);

        append(
            &path,
            1,
            &WalEntry::Experience {
                episode: Episode {
                    content: "wal replay test".into(),
                    context: "test".into(),
                    outcome: None,
                    salience_hint: 0.6,
                    semantic_vector: None,
                    agent_id: None,
                    tenant_id: None,
                    rag: None,
                provenance: None,
                },
            },
        )
        .unwrap();

        let fresh = FluctlightBrain::open(&path).unwrap();
        assert!(fresh.activate("wal replay").recalls.len() >= 1);
    }

    #[test]
    fn wal_skips_corrupt_line() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("test.flct");
        let mut brain = FluctlightBrain::open(&path).unwrap();
        brain.checkpoint().unwrap();
        let wal = active_segment(&path);
        {
            let mut f = OpenOptions::new().create(true).append(true).open(&wal).unwrap();
            writeln!(f, "{{not valid json").unwrap();
            let good = WalRecord {
                seq: 1,
                idempotency_key: None,
                entry: WalEntry::Experience {
                    episode: Episode {
                        content: "after corrupt".into(),
                        context: "t".into(),
                        outcome: None,
                        salience_hint: 0.5,
                        semantic_vector: None,
                        agent_id: None,
                        tenant_id: None,
                        rag: None,
                provenance: None,
                    },
                },
            };
            writeln!(f, "{}", serde_json::to_string(&good).unwrap()).unwrap();
        }
        drop(brain);
        let replay_brain = FluctlightBrain::open(&path).unwrap();
        assert!(replay_brain.activate("after corrupt").recalls.len() >= 1);
    }
}
