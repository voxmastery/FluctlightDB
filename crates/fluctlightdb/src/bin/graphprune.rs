//! Offline graph maintenance: weight histogram + prune to a target synapse count.
//! Usage: graphprune <brain-dir> [target_count]   (no target = histogram only)
use std::path::PathBuf;

fn main() {
    let dir = PathBuf::from(std::env::args().nth(1).expect("usage: graphprune <brain-dir> [target]"));
    let target: Option<usize> = std::env::args().nth(2).and_then(|s| s.parse().ok());

    let t0 = std::time::Instant::now();
    let mut brain = fluctlightdb::FluctlightBrain::open(&dir).expect("open brain");
    println!("loaded in {:?}; synapses={}", t0.elapsed(), brain.graph.synapse_count());

    // weight histogram
    let mut buckets = [0usize; 10];
    for s in &brain.graph.synapses {
        let b = ((s.weight.clamp(0.0, 0.9999)) * 10.0) as usize;
        buckets[b] += 1;
    }
    for (i, n) in buckets.iter().enumerate() {
        println!("  weight [{:.1}-{:.1}): {:>10}", i as f32 / 10.0, (i + 1) as f32 / 10.0, n);
    }

    let Some(target) = target else { return };
    // target >= 1000: keep exactly the strongest `target` synapses (ties split arbitrarily —
    // the tied mass at weight 1.0 is saturation artifacts from the old global-sweep bug).
    if target >= 1000 {
        let mut weights: Vec<f32> = brain.graph.synapses.iter().map(|s| s.weight).collect();
        weights.sort_unstable_by(|a, b| b.partial_cmp(a).unwrap());
        let thr = weights[target.min(weights.len()) - 1];
        let above: usize = brain.graph.synapses.iter().filter(|s| s.weight > thr).count();
        let mut at_budget = target - above;
        let t1 = std::time::Instant::now();
        brain.graph.synapses.retain(|s| {
            if s.weight > thr { true }
            else if s.weight == thr && at_budget > 0 { at_budget -= 1; true }
            else { false }
        });
        brain.graph.rebuild_index();
        println!("kept top {} (thr {thr:.4}) in {:?}; now {}", target, t1.elapsed(), brain.graph.synapse_count());
        let t2 = std::time::Instant::now();
        brain.save().expect("save");
        println!("checkpointed in {:?}", t2.elapsed());
        return;
    }
    // arg < 1000 is a direct weight-threshold*1000 (e.g. 300 = prune below 0.300);
    // otherwise it is a target synapse count found by binary search.
    if target < 1000 {
        let thr = target as f32 / 1000.0;
        let t1 = std::time::Instant::now();
        let pruned = brain.graph.prune_below(thr);
        println!("pruned {pruned} below {thr:.3} in {:?}; now {}", t1.elapsed(), brain.graph.synapse_count());
        let t2 = std::time::Instant::now();
        brain.save().expect("save");
        println!("checkpointed in {:?}", t2.elapsed());
        return;
    }
    // binary-search a threshold that lands under target
    let mut lo = 0.0f32;
    let mut hi = 1.0f32;
    for _ in 0..24 {
        let mid = (lo + hi) / 2.0;
        let keep = brain.graph.synapses.iter().filter(|s| s.weight >= mid).count();
        if keep > target { lo = mid } else { hi = mid }
    }
    let thr = hi;
    let keep = brain.graph.synapses.iter().filter(|s| s.weight >= thr).count();
    println!("pruning below weight {thr:.4} -> keeping {keep}");
    let t1 = std::time::Instant::now();
    let pruned = brain.graph.prune_below(thr);
    println!("pruned {pruned} in {:?}; now {}", t1.elapsed(), brain.graph.synapse_count());
    let t2 = std::time::Instant::now();
    brain.save().expect("save");
    println!("checkpointed in {:?}", t2.elapsed());
}
