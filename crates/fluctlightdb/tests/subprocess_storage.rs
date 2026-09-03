use std::fs;
use std::process::{Child, Command, ExitStatus};
use std::thread;
use std::time::{Duration, Instant};

use fluctlightdb::placement::DurabilityPolicy;
use fluctlightdb::store_lock::StoreLock;
use fluctlightdb::wal::{self, WalEntry, WalIdentity};
use fluctlightdb::{Episode, FluctlightBrain};

fn child(test: &str, path: &std::path::Path) -> Command {
    let mut command = Command::new(std::env::current_exe().unwrap());
    command
        .args(["--ignored", "--exact", test])
        .env("FLUCTLIGHT_SUBPROCESS_BRAIN", path);
    command
}

fn wait_for(path: &std::path::Path) {
    let deadline = Instant::now() + Duration::from_secs(5);
    while !path.exists() {
        assert!(Instant::now() < deadline, "timed out waiting for {path:?}");
        thread::sleep(Duration::from_millis(10));
    }
}

fn success(status: ExitStatus) {
    assert!(status.success(), "subprocess failed: {status}");
}

#[test]
#[ignore = "subprocess helper"]
fn writer_holder_child() {
    let path = std::env::var_os("FLUCTLIGHT_SUBPROCESS_BRAIN").unwrap();
    let path = std::path::Path::new(&path);
    let _brain = FluctlightBrain::open(path).unwrap();
    fs::write(path.with_extension("ready"), b"ready").unwrap();
    wait_for(&path.with_extension("release"));
}

#[test]
#[ignore = "subprocess helper"]
fn idle_writer_child() {
    let path = std::env::var_os("FLUCTLIGHT_SUBPROCESS_BRAIN").unwrap();
    let mut brain = FluctlightBrain::open(std::path::Path::new(&path)).unwrap();
    brain
        .experience(Episode::new("idle durable mutation", "phase5", 0.9))
        .unwrap();
    thread::sleep(Duration::from_millis(100));
}

#[test]
#[ignore = "subprocess helper"]
fn readonly_replay_child() {
    let path = std::env::var_os("FLUCTLIGHT_SUBPROCESS_BRAIN").unwrap();
    let brain = FluctlightBrain::open_readonly(std::path::Path::new(&path)).unwrap();
    assert!(brain.checkpoint().is_err());
}

#[test]
#[ignore = "subprocess helper"]
fn stale_fence_child() {
    let path = std::env::var_os("FLUCTLIGHT_SUBPROCESS_BRAIN").unwrap();
    let stale = WalIdentity {
        tenant_uuid: uuid::Uuid::from_u128(501),
        writer_epoch: 1,
        fence_generation: 8,
        durability: DurabilityPolicy::Quorum,
    };
    assert!(wal::append_fenced(
        std::path::Path::new(&path),
        2,
        &WalEntry::Tick { n: 1 },
        &stale
    )
    .is_err());
}

#[test]
#[ignore = "subprocess helper"]
fn rejected_wal_child() {
    let path = std::env::var_os("FLUCTLIGHT_SUBPROCESS_BRAIN").unwrap();
    assert!(FluctlightBrain::open(std::path::Path::new(&path)).is_err());
}

#[test]
fn concurrent_writer_is_rejected_across_processes() {
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("brain");
    let mut holder: Child = child("writer_holder_child", &path).spawn().unwrap();
    wait_for(&path.with_extension("ready"));

    assert!(
        StoreLock::try_acquire(&path).is_err(),
        "second process must not acquire the writer fence"
    );

    fs::write(path.with_extension("release"), b"release").unwrap();
    success(holder.wait().unwrap());
}

#[test]
fn idle_acknowledged_mutation_is_durable_after_process_exit() {
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("brain");
    success(child("idle_writer_child", &path).status().unwrap());
    let reopened = FluctlightBrain::open(&path).unwrap();
    assert!(reopened
        .activate("idle durable mutation")
        .recalls
        .iter()
        .any(|recall| recall.episode.content == "idle durable mutation"));
}

#[test]
fn readonly_replica_replay_never_appends_or_checkpoints() {
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("brain");
    let writer = FluctlightBrain::open(&path).unwrap();
    writer.checkpoint().unwrap();
    drop(writer);
    wal::append(&path, 1, &WalEntry::Tick { n: 1 }).unwrap();
    let wal_path = wal::wal_path(&path);
    let before = fs::read(&wal_path).unwrap();

    success(child("readonly_replay_child", &path).status().unwrap());

    assert_eq!(fs::read(wal_path).unwrap(), before);
}

#[test]
fn stale_fence_is_rejected_across_processes() {
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("fenced.brain");
    let current = WalIdentity {
        tenant_uuid: uuid::Uuid::from_u128(501),
        writer_epoch: 1,
        fence_generation: 9,
        durability: DurabilityPolicy::Quorum,
    };
    wal::append_fenced(&path, 1, &WalEntry::Tick { n: 1 }, &current).unwrap();
    success(child("stale_fence_child", &path).status().unwrap());
}

#[test]
fn wal_gap_and_interior_corruption_are_rejected_in_fresh_processes() {
    for corruption in ["gap", "interior"] {
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join(format!("{corruption}.brain"));
        let brain = FluctlightBrain::open(&path).unwrap();
        brain.checkpoint().unwrap();
        drop(brain);
        match corruption {
            "gap" => wal::append(&path, 2, &WalEntry::Tick { n: 1 }).unwrap(),
            "interior" => {
                wal::append(&path, 1, &WalEntry::Tick { n: 1 }).unwrap();
                fs::write(wal::wal_path(&path), b"{broken}\n{\"seq\":2}\n").unwrap();
            }
            _ => unreachable!(),
        }
        success(child("rejected_wal_child", &path).status().unwrap());
    }
}

#[test]
fn wal_segments_are_replayed_in_numeric_not_lexical_order() {
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("numeric.flct");
    let base = wal::wal_base(&path);
    fs::write(format!("{}.10", base.display()), b"ten").unwrap();
    fs::write(format!("{}.2", base.display()), b"two").unwrap();
    let names: Vec<_> = wal::list_segments(&path)
        .into_iter()
        .map(|path| path.file_name().unwrap().to_string_lossy().into_owned())
        .collect();
    assert_eq!(names, ["numeric.flct.wal.2", "numeric.flct.wal.10"]);
}
