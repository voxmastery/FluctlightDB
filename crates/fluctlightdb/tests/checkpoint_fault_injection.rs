use std::fs;
use std::process::Command;

use fluctlightdb::checkpoint_fault::{CHECKPOINT_CRASH_EXIT_CODE, CHECKPOINT_FAULT_POINTS};
use fluctlightdb::manifest::{load_v4_dir, save_v4_dir};
use fluctlightdb::{Episode, FluctlightBrain};

const CHILD_TEST: &str = "checkpoint_fault_child";

fn brain_with(content: &str) -> FluctlightBrain {
    let mut brain = FluctlightBrain::new();
    brain
        .experience(Episode::new(content, "checkpoint-fault", 0.9))
        .unwrap();
    brain
}

#[test]
#[ignore = "subprocess helper"]
fn checkpoint_fault_child() {
    let path = std::env::var_os("FLUCTLIGHT_FAULT_BRAIN").unwrap();
    let path = std::path::Path::new(&path);
    let point = std::env::var("FLUCTLIGHT_CHECKPOINT_CRASH_AT").unwrap();
    if point.starts_with("wal.") {
        let mut brain = FluctlightBrain::open(path).unwrap();
        brain
            .experience(Episode::new("new generation", "checkpoint-fault", 0.9))
            .unwrap();
        brain.checkpoint().unwrap();
    } else {
        save_v4_dir(&brain_with("new generation"), path).unwrap();
    }
}

#[test]
fn every_checkpoint_crash_point_reopens_exactly_old_or_new() {
    for point in CHECKPOINT_FAULT_POINTS {
        let root = tempfile::tempdir().unwrap();
        let brain_path = root.path().join("brain");
        save_v4_dir(&brain_with("old generation"), &brain_path).unwrap();
        let old_current = fs::read_to_string(brain_path.join("CURRENT")).unwrap();

        let status = Command::new(std::env::current_exe().unwrap())
            .args(["--ignored", "--exact", CHILD_TEST])
            .env("FLUCTLIGHT_FAULT_BRAIN", &brain_path)
            .env("FLUCTLIGHT_ENABLE_FAULT_INJECTION", "1")
            .env("FLUCTLIGHT_CHECKPOINT_CRASH_AT", point)
            .status()
            .unwrap();
        assert_eq!(
            status.code(),
            Some(CHECKPOINT_CRASH_EXIT_CODE),
            "fault point {point} was not reached"
        );

        let reopened = load_v4_dir(&brain_path).unwrap();
        let contents: Vec<_> = reopened
            .hippocampus
            .engrams
            .iter()
            .map(|engram| engram.episode.content.as_str())
            .collect();
        assert!(
            contents == ["old generation"]
                || contents == ["old generation", "new generation"]
                || contents == ["new generation"],
            "fault point {point} reopened a mixed generation: {contents:?}"
        );
        let current = fs::read_to_string(brain_path.join("CURRENT")).unwrap();
        let current_name = current.trim();
        assert!(
            current_name.starts_with("gen-")
                && current_name.len() == 24
                && brain_path.join("generations").join(current_name).is_dir(),
            "CURRENT must name a complete generation: {current:?}"
        );
        assert!(
            current == old_current || current_name.ends_with("00000000000000000002"),
            "CURRENT must be exactly the old or new publication: {current:?}"
        );
    }
}
