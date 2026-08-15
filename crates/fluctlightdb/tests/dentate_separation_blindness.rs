//! Regression tests for the two consumers that read `dg_neurons` as if it were a pure
//! content code, when it is a content code plus per-engram-unique separator neurons that
//! the dentate gyrus fabricated at encode time.
//!
//! Both bugs are invisible from outside: the compactor silently under-merges, and the
//! ingest gate silently admits duplicates. Neither errors.

use fluctlightdb::test_env::EnvGuard;
use fluctlightdb::types::Episode;
use fluctlightdb::FluctlightBrain;

const GATE: &[&str] = &["FLUCTLIGHT_SEPARATION_GATE"];

// These tests disagree about whether the ingest gate should be on, and the gate is
// configured through a process-global environment variable — so they must not run
// concurrently. `EnvGuard` takes a process-wide lock and restores the prior value on drop.
// (That this is necessary at all is the configuration defect these fixes do not address.)

/// `compact::should_merge` keyed on `jaccard(a.dg_neurons, b.dg_neurons) > 0.85`.
/// The separators live in that set and are unique per engram by construction, so they
/// depress every score by a fixed amount — capping near-duplicates below the threshold
/// exactly where compaction is supposed to act.
///
/// Measured on this corpus: 1 merge before the fix, 7 after.
#[test]
fn compactor_merges_near_duplicates_despite_separators() {
    let env = EnvGuard::acquire(GATE);
    env.set("FLUCTLIGHT_SEPARATION_GATE", "0");
    let mut brain = FluctlightBrain::new();
    // Unique content (one differing token) so the "exactly equal content+context"
    // short-circuit cannot fire and the decision truly rests on the neuron Jaccard.
    for i in 0..120 {
        brain
            .experience(Episode::new(
                format!("the deployment pipeline failed during the release step on host {i}"),
                "prod",
                0.5,
            ))
            .unwrap();
    }
    let report = brain.compact().unwrap();
    assert!(
        report.merged_engrams >= 5,
        "compaction should merge near-identical engrams; separator pollution held this to 1. \
         got {}",
        report.merged_engrams
    );
}

/// `separation_gate::assess` scored a candidate with
/// `best_sep = peer.separation_index.max(1.0 - jaccard)`.
///
/// `separation_index` records how well the DG managed to orthogonalise that peer *at its own
/// encode time*. A peer written while it was novel carries an index near 1.0 — and that 1.0
/// was then handed to any later near-duplicate as if it described the new pair. The `.max()`
/// meant the gate could never reject anything that collided with a once-novel memory, which
/// is the common case. Measuring separation against the peer's clean content code instead
/// asks the right question: how far apart are these two *contents*?
#[test]
fn gate_does_not_credit_a_peers_prior_novelty() {
    let env = EnvGuard::acquire(GATE);
    env.set("FLUCTLIGHT_SEPARATION_GATE", "1");
    let shared: Vec<String> = (0..17).map(|i| format!("tok{i}")).collect();
    let first = format!("{} alpha bravo charlie", shared.join(" "));
    let dup = format!("{} delta echo foxtrot", shared.join(" "));

    let mut brain = FluctlightBrain::new();
    let r1 = brain.experience(Episode::new(first, "ops", 0.5)).unwrap();
    assert!(
        !r1.gate_rejected,
        "precondition: the first write is admitted"
    );
    let peer_sep = brain.hippocampus.engrams[0].separation_index;
    assert!(
        peer_sep > 0.9,
        "precondition: a peer written while novel carries a high separation_index \
         (got {peer_sep}) — this is the value the gate used to donate to duplicates"
    );

    // 17 shared tokens vs 3 differing each way => Jaccard 17/23 ~= 0.74, inside the
    // 0.72..0.85 judgement band where the gate's scoring actually decides the outcome.
    let report = brain.experience(Episode::new(dup, "ops", 0.5)).unwrap();
    assert!(
        report.confusion_risk >= 0.72 && report.confusion_risk < 0.85,
        "precondition: overlap must land in the judgement band, got {}",
        report.confusion_risk
    );
    assert!(
        report.gate_rejected,
        "a near-duplicate must not inherit the peer's prior novelty \
         (confusion_risk={}, reason={:?})",
        report.confusion_risk, report.gate_reason
    );
}

/// A genuinely novel memory must still be admitted — the un-blinded gate must not
/// become a blanket reject.
#[test]
fn gate_still_admits_novel_content() {
    let env = EnvGuard::acquire(GATE);
    env.set("FLUCTLIGHT_SEPARATION_GATE", "1");
    let mut brain = FluctlightBrain::new();
    for i in 0..10 {
        let _ = brain.experience(Episode::new(
            format!("quarterly revenue report northern region tier {i}"),
            "finance",
            0.5,
        ));
    }
    let novel = brain
        .experience(Episode::new(
            "the customer cancelled their subscription after a billing dispute",
            "support",
            0.7,
        ))
        .unwrap();
    assert!(
        !novel.gate_rejected,
        "unrelated content must still be admitted: {:?}",
        novel.gate_reason
    );
}
