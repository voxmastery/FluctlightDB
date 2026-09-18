//! Whole-brain FlyWire v783 perf gate. Skipped unless FLUCTLIGHT_CONNECTOME_DIR points at a
//! directory holding connections.csv / neurons.csv / classification.csv (Codex v783, CC-BY-4.0):
//!   B=https://storage.googleapis.com/flywire-data/codex/data/fafb/783
//!   for f in connections neurons classification; do curl -sO $B/$f.csv.gz && gunzip -f $f.csv.gz; done
//! Run: FLUCTLIGHT_CONNECTOME_DIR=/path cargo test -p fluctlightdb --release --test connectome_perf -- --ignored --nocapture

use fluctlightdb::test_env::EnvGuard;
use fluctlightdb::{import_connectome, FluctlightBrain, ImportConfig};
use std::path::PathBuf;
use std::time::Instant;
use tempfile::tempdir;

fn rss_mb() -> u64 {
    std::fs::read_to_string("/proc/self/status")
        .ok()
        .and_then(|s| s.lines().find(|l| l.starts_with("VmRSS:")).and_then(|l| l.split_whitespace().nth(1)?.parse::<u64>().ok()))
        .map(|kb| kb / 1024)
        .unwrap_or(0)
}

#[test]
#[ignore]
fn whole_brain_v783_within_budget() {
    let Some(dir) = std::env::var_os("FLUCTLIGHT_CONNECTOME_DIR") else {
        eprintln!("FLUCTLIGHT_CONNECTOME_DIR unset — skipping");
        return;
    };
    let dir = PathBuf::from(dir);
    let _g = EnvGuard::acquire(&["FLUCTLIGHT_STORAGE", "FLUCTLIGHT_SOMNUS"]);
    std::env::remove_var("FLUCTLIGHT_SOMNUS");
    std::env::set_var("FLUCTLIGHT_STORAGE", "v4");

    let tmp = tempdir().unwrap();
    let path = tmp.path().join("fly-783");
    let cfg = ImportConfig {
        connections: dir.join("connections.csv"),
        classification: dir.join("classification.csv"),
        neurons: dir.join("neurons.csv"),
        ..Default::default()
    };

    let rss0 = rss_mb();
    let t = Instant::now();
    let mut brain = FluctlightBrain::open(&path).unwrap();
    let report = import_connectome(&mut brain, &cfg).unwrap();
    brain.checkpoint().unwrap();
    let total_s = t.elapsed().as_secs_f64();
    let peak_mb = rss_mb() - rss0;

    let t = Instant::now();
    let r = brain.activate("odor sugar reward");
    let recall_ms = t.elapsed().as_secs_f64() * 1000.0;

    eprintln!("{report:#?}\ntotal_s={total_s:.2} peak_mb={peak_mb} recall_ms={recall_ms:.2} active={} seeds={:?}", r.active_neurons, r.connectome_seeds);
    assert_eq!(report.entry_set_len, 5177, "Kenyon cells in v783");
    assert!(report.pairs > 2_600_000 && report.pairs < 2_800_000, "{}", report.pairs);
    assert!(total_s <= 20.0, "import+checkpoint {total_s:.1}s > 20s");
    assert!(peak_mb <= 1024, "peak RSS {peak_mb} MB > 1 GB");
    assert!(recall_ms <= 20.0, "4-hop recall {recall_ms:.1} ms > 20 ms");
    assert_eq!(r.connectome_seeds, Some(3 * 7));
}
