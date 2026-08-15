//! Jepsen-style chaos tests for embedded brain durability.
//!
//! FluctlightDB is not a distributed consensus database (no Raft cluster).
//! This suite models **real failure modes**: kill -9 mid-write, torn WAL,
//! replicate during partial WAL, and verify brain integrity after recovery.
//!
//! Run: `cargo test --test chaos_jepsen --release`
//! Or:  `bash scripts/jepsen-chaos.sh`

use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::Path;
use std::process::{Command, Stdio};
use std::sync::mpsc;
use std::thread;
use std::time::Duration;

use fluctlightdb::brain::FluctlightBrain;
use fluctlightdb::manifest::save_v4_dir;
use fluctlightdb::replicate::{open_replica_brain, sync_once};
use fluctlightdb::store::verify_path;
use fluctlightdb::types::Episode;
use fluctlightdb::wal::{self, WalEntry};
use tempfile::tempdir;

fn episode(content: &str) -> Episode {
    Episode {
        content: content.into(),
        context: "chaos".into(),
        outcome: None,
        salience_hint: 0.6,
        semantic_vector: None,
        agent_id: None,
        tenant_id: None,
        rag: None,
        provenance: None,
    }
}

fn wal_append(path: &Path, seq: u64, content: &str) {
    wal::append(
        path,
        seq,
        &WalEntry::Experience {
            episode: episode(content),
            assigned_engram_id: None,
        },
    )
    .expect("wal append");
}

fn tear_wal_tail(path: &Path) {
    let wal = wal::wal_path(path);
    if !wal.exists() {
        wal_append(path, 1, "seed before tear");
    }
    let mut f = OpenOptions::new().append(true).open(&wal).unwrap();
    f.write_all(b"{\"seq\":999,\"op\":\"experience\",\"episode\":{")
        .unwrap();
    f.sync_all().unwrap();
}

fn assert_brain_ok(path: &Path) {
    let report = verify_path(path).expect("verify_path");
    assert!(report.ok, "brain corrupt: {:?}", report);
}

#[test]
fn chaos_property_rounds_checkpoint_wal_tear_and_reopen() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("roundtrip.flct");

    for round in 0..8 {
        let mut brain = FluctlightBrain::open(&path).unwrap();
        brain
            .experience(episode(&format!("round {round} alpha")))
            .unwrap();
        if round % 2 == 0 {
            brain.checkpoint().unwrap();
        }
        let next_wal_seq = brain.wal_seq.saturating_add(1);
        drop(brain);

        if round % 3 == 0 {
            wal_append(
                &path,
                next_wal_seq,
                &format!("round {round} wal-only"),
            );
        }
        if round % 4 == 0 {
            tear_wal_tail(&path);
        }

        let loaded = FluctlightBrain::open(&path).unwrap();
        assert!(
            loaded.hippocampus.engrams.len() >= 1,
            "round {round}: expected recoverable engrams"
        );
        assert_brain_ok(&path);
    }
}

#[test]
fn chaos_replicate_primary_with_torn_wal_replica_still_loads() {
    let dir = tempdir().unwrap();
    let primary = dir.path().join("primary");
    let replica = dir.path().join("replica");

    let mut brain = FluctlightBrain::new();
    brain.experience(episode("replicate baseline")).unwrap();
    save_v4_dir(&brain, &primary).unwrap();
    drop(brain);

    wal_append(&primary, 1, "post-snapshot wal line");
    tear_wal_tail(&primary);

    let status = sync_once(&primary, &replica).expect("sync_once");
    assert!(status.wal_segments >= 1, "expected WAL segments copied");

    let loaded = open_replica_brain(&replica).expect("replica open");
    assert!(
        loaded.hippocampus.engrams.len() >= 1,
        "replica should load after primary WAL tear"
    );
}

#[test]
fn chaos_exclusive_lock_held_across_thread_releases() {
    use fluctlightdb::store_lock::StoreLock;

    let dir = tempdir().unwrap();
    let brain = dir.path().join("brain");
    fs::create_dir_all(&brain).unwrap();

    let (tx, rx) = mpsc::channel();
    let brain_path = brain.clone();
    let holder = thread::spawn(move || {
        let _lock = StoreLock::try_acquire(&brain_path).unwrap();
        tx.send(()).unwrap();
        thread::sleep(Duration::from_millis(150));
    });

    rx.recv().unwrap();
    assert!(
        StoreLock::try_acquire(&brain).is_err(),
        "writer lock should block second acquirer"
    );
    holder.join().unwrap();
    assert!(
        StoreLock::try_acquire(&brain).is_ok(),
        "lock should release after holder drops"
    );
}

/// Subprocess writer killed with SIGKILL (Unix). Parent reopens brain — WAL must
/// recover committed prefix or reject torn tail without corrupting store.
#[cfg(unix)]
#[test]
fn chaos_subprocess_sigkill_mid_write() {
    use std::os::unix::process::ExitStatusExt;

    let dir = tempdir().unwrap();
    let path = dir.path().join("kill9.flct");
    let path_arg = path.to_string_lossy().to_string();

    // Seed checkpoint so WAL replay path is exercised after kill.
    let brain = FluctlightBrain::open(&path).unwrap();
    brain.checkpoint().unwrap();
    drop(brain);

    let exe = env!("CARGO_BIN_EXE_fluctlight-chaos-worker");
    let mut child = Command::new(exe)
        .arg(&path_arg)
        .arg("24")
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .expect("spawn chaos worker");

    thread::sleep(Duration::from_millis(80));
    unsafe {
        libc::kill(child.id() as i32, libc::SIGKILL);
    }
    let status = child.wait().expect("wait child");
    assert!(
        status.signal() == Some(9) || !status.success(),
        "child should be killed or fail: {status:?}"
    );

    let loaded = FluctlightBrain::open(&path).expect("reopen after kill");
    assert!(
        loaded.hippocampus.engrams.len() >= 1,
        "expected at least one recovered engram after SIGKILL"
    );
    assert_brain_ok(&path);
}

#[cfg(not(unix))]
#[test]
fn chaos_subprocess_sigkill_mid_write() {
    // Windows: use crash_recovery unit tests; full SIGKILL harness is Unix-only.
}

#[test]
fn chaos_verify_rejects_truncated_brain_file() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("trunc.flct");
    let mut brain = FluctlightBrain::open(&path).unwrap();
    brain.experience(episode("trunc test")).unwrap();
    brain.checkpoint().unwrap();
    drop(brain);

    let meta = fs::metadata(&path).unwrap();
    let truncate_to = meta.len().saturating_sub(64).max(32);
    let bytes = fs::read(&path).unwrap();
    fs::write(&path, &bytes[..truncate_to as usize]).unwrap();

    let report = verify_path(&path).unwrap();
    assert!(!report.ok, "truncated brain should fail verify");
}
