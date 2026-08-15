//! Adversarial crash / torn-write scenarios for WAL + brain reopen.

use std::fs::{self, OpenOptions};
use std::io::Write;

use fluctlightdb::brain::FluctlightBrain;
use fluctlightdb::types::Episode;
use fluctlightdb::wal::{self, WalEntry};
use tempfile::tempdir;

fn sample_episode(content: &str) -> Episode {
    Episode {
        content: content.into(),
        context: "crash-test".into(),
        outcome: None,
        salience_hint: 0.55,
        semantic_vector: None,
        agent_id: None,
        tenant_id: None,
        rag: None,
        provenance: None,
    }
}

fn append_experience(path: &std::path::Path, seq: u64, content: &str) {
    wal::append(
        path,
        seq,
        &WalEntry::Experience {
            episode: sample_episode(content),
            assigned_engram_id: None,
        },
    )
    .expect("wal append");
}

#[test]
fn crash_recovery_replays_wal_after_checkpoint() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("brain.flct");
    let brain = FluctlightBrain::open(&path).unwrap();
    brain.checkpoint().unwrap();
    drop(brain);

    append_experience(&path, 1, "survives crash replay");

    let loaded = FluctlightBrain::open(&path).unwrap();
    assert!(
        loaded.activate("survives crash").recalls.len() >= 1,
        "expected WAL replay after simulated crash"
    );
}

#[test]
fn crash_recovery_rejects_interior_corrupt_wal_line() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("brain.flct");
    let mut brain = FluctlightBrain::open(&path).unwrap();
    brain.checkpoint().unwrap();
    let wal = wal::wal_path(&path);
    {
        let mut f = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&wal)
            .unwrap();
        writeln!(f, "{{not valid json").unwrap();
    }
    drop(brain);
    append_experience(&path, 1, "after corrupt line");

    let error = match FluctlightBrain::open(&path) {
        Ok(_) => panic!("interior WAL corruption must not be silently skipped"),
        Err(error) => error,
    };
    assert!(error.to_string().contains("interior WAL corruption"));
}

#[test]
fn crash_recovery_truncated_wal_tail() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("brain.flct");
    let mut brain = FluctlightBrain::open(&path).unwrap();
    brain.checkpoint().unwrap();
    drop(brain);
    append_experience(&path, 1, "before torn write");
    let wal = wal::wal_path(&path);
    {
        let mut f = OpenOptions::new().append(true).open(&wal).unwrap();
        f.write_all(b"{\"seq\":2,\"entry\":{\"Experience\":")
            .unwrap();
        f.sync_all().unwrap();
    }

    let loaded = FluctlightBrain::open(&path).unwrap();
    assert!(loaded.activate("before torn").recalls.len() >= 1);
}

#[test]
fn crash_recovery_uncheckpointed_experience_persists_via_wal() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("brain.flct");
    let mut brain = FluctlightBrain::open(&path).unwrap();
    brain
        .experience(sample_episode("mid-flight before checkpoint"))
        .unwrap();
    drop(brain);

    let loaded = FluctlightBrain::open(&path).unwrap();
    assert!(
        loaded.activate("mid-flight").recalls.len() >= 1,
        "uncheckpointed experience should replay from WAL"
    );
}

#[test]
fn crash_recovery_empty_wal_segment_is_harmless() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("brain.flct");
    let brain = FluctlightBrain::open(&path).unwrap();
    brain.checkpoint().unwrap();
    let wal = wal::wal_path(&path);
    fs::write(&wal, b"").unwrap();
    drop(brain);

    let loaded = FluctlightBrain::open(&path).unwrap();
    assert_eq!(loaded.hippocampus.engrams.len(), 0);
}
