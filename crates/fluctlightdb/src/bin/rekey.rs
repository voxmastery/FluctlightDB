//! Offline codec migration: open a brain, drain the entire re-key queue, checkpoint.
//!
//! Run with the serve stopped — open takes the exclusive store lock. This is the
//! operator path for a 0.5.19 -> 0.5.21 cutover: the queue would otherwise drain at
//! 4 per write / 128 per sleep, leaving the brain mid-migration for days.
//!
//! Usage: fluctlight-rekey <brain-path>

use fluctlightdb::FluctlightBrain;

fn main() {
    let path = match std::env::args().nth(1) {
        Some(p) => std::path::PathBuf::from(p),
        None => {
            eprintln!("usage: fluctlight-rekey <brain-path>");
            std::process::exit(2);
        }
    };
    let start = std::time::Instant::now();
    let mut brain = match FluctlightBrain::open(&path) {
        Ok(b) => b,
        Err(e) => {
            eprintln!("open failed: {e}");
            std::process::exit(1);
        }
    };
    let status = brain.status();
    println!(
        "opened: engrams={} codec={} rekey_pending={}",
        status.engrams, status.neuron_codec, status.rekey_pending
    );
    if status.rekey_pending == 0 {
        println!("nothing to do");
        return;
    }
    // Weight-preserving whole-brain migration; falls back to the re-derive drain
    // only for a drifted brain whose old ids are no longer reproducible.
    match fluctlightdb::derive::migrate_codec(&mut brain) {
        Ok(report) => println!(
            "migrated in {:.1}s: {}",
            start.elapsed().as_secs_f32(),
            serde_json::to_string(&report).unwrap()
        ),
        Err(e) => {
            eprintln!("map-based migration unavailable ({e}); falling back to re-key drain");
            let done = brain.rekey_now();
            println!(
                "re-keyed {done} engrams in {:.1}s",
                start.elapsed().as_secs_f32()
            );
        }
    }
    let status = brain.status();
    println!(
        "post-migration: codec={} rekey_pending={}",
        status.neuron_codec, status.rekey_pending
    );
    if status.rekey_pending != 0 {
        eprintln!("queue did not drain fully — aborting without checkpoint");
        std::process::exit(1);
    }
    if let Err(e) = brain.checkpoint() {
        eprintln!("checkpoint failed: {e}");
        std::process::exit(1);
    }
    println!(
        "checkpoint written; total {:.1}s",
        start.elapsed().as_secs_f32()
    );
}
