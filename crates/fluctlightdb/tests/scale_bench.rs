//! Scale recall benchmark — 10k engrams, LongMemEval-style associative bar.
//! Run: FLUCTLIGHT_SCALE_BENCH=1 cargo test -p fluctlightdb scale_recall_10k -- --ignored --nocapture

use std::io::Write;
use std::path::PathBuf;
use std::time::Instant;

use fluctlightdb::development::DevStage;
use fluctlightdb::{Episode, FluctlightBrain};

fn vec_for(i: usize, dim: usize) -> Vec<f32> {
    let mut out = vec![0.0f32; dim];
    out[i % dim] = 0.92;
    out[(i * 7 + 13) % dim] = 0.38;
    let n: f32 = out.iter().map(|x| x * x).sum::<f32>().sqrt();
    if n > 0.0 {
        for x in &mut out {
            *x /= n;
        }
    }
    out
}

fn bench_progress(msg: &str) {
    let mut stderr = std::io::stderr().lock();
    let _ = writeln!(stderr, "{msg}");
    let _ = stderr.flush();
}

#[test]
#[ignore = "scale bench: set FLUCTLIGHT_SCALE_BENCH=1"]
fn scale_recall_10k() {
    if std::env::var("FLUCTLIGHT_SCALE_BENCH").ok().as_deref() != Some("1") {
        return;
    }
    // Bulk ingest: WAL durability, amortized snapshots, no per-save verify/backup.
    std::env::set_var("FLUCTLIGHT_CHECKPOINT_EVERY_N", "100000");
    std::env::set_var("FLUCTLIGHT_SAVE_VERIFY", "0");
    std::env::set_var("FLUCTLIGHT_SAVE_BACKUP", "0");
    std::env::set_var("FLUCTLIGHT_WAL", "0");
    std::env::set_var("FLUCTLIGHT_SEPARATION_OVERLAP_WINDOW", "256");

    let n: usize = std::env::var("FLUCTLIGHT_SCALE_N")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(10_000);
    let target: f64 = std::env::var("FLUCTLIGHT_SCALE_TARGET")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(0.85);

    let brain_path = std::env::var("FLUCTLIGHT_SCALE_BRAIN")
        .map(PathBuf::from)
        .unwrap_or_else(|_| {
            let dir = tempfile::tempdir().unwrap();
            dir.path().join("brain")
        });
    if !brain_path.exists() {
        std::fs::create_dir_all(&brain_path).unwrap();
    }

    let mut brain = FluctlightBrain::new();
    brain.development.stage = DevStage::Expert;
    brain.attach_store_path(brain_path.clone());

    let dim = 384;
    let ingest_start = Instant::now();
    for i in 0..n {
        let content = format!(
            "incident {i}: database connection pool exhausted on service-{}",
            i % 50
        );
        brain
            .experience(Episode {
                content,
                context: "scale-bench".into(),
                outcome: None,
                salience_hint: 0.72,
                semantic_vector: Some(vec_for(i, dim)),
                agent_id: None,
                tenant_id: None,
                rag: None,
                provenance: None,
            })
            .unwrap();
        if i > 0 && i % 2000 == 0 {
            bench_progress(&format!(
                "ingested {i}/{n} engrams ({:.1}s)...",
                ingest_start.elapsed().as_secs_f64()
            ));
        }
    }
    brain.checkpoint().unwrap();
    bench_progress(&format!(
        "ingest done: {n} engrams in {:.1}s",
        ingest_start.elapsed().as_secs_f64()
    ));
    assert!(
        brain.has_sidecar_index(),
        "FTS5+HNSW sidecar required at scale"
    );

    let query_n = (n / 10).max(100).min(1000);
    let mut hits = 0usize;
    let query_start = Instant::now();
    for q in 0..query_n {
        let i = q * (n / query_n);
        let cue = format!("connection pool exhausted service-{}", i % 50);
        let r = brain.activate_with_semantic(&cue, Some(&vec_for(i, dim)));
        if !r.recalls.is_empty() {
            hits += 1;
        }
    }
    let rate = hits as f64 / query_n as f64;
    bench_progress(&format!(
        "scale recall: {hits}/{query_n} = {:.1}% (target {:.0}%, engrams={n}, sidecar={}, query {:.1}s)",
        rate * 100.0,
        target * 100.0,
        brain.has_sidecar_index(),
        query_start.elapsed().as_secs_f64()
    ));
    assert!(
        rate >= target,
        "recall {rate:.1}% below industrial target {target:.0}%"
    );
}
