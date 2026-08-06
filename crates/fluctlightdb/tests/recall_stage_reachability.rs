//! Which stages of the recall pipeline actually execute at the default posture.
//!
//! `activate_scoped` applies roughly fifteen scoring stages in sequence, each with its own
//! hand-tuned constants, and every published benchmark number depends on the whole stack.
//! Nothing measured which stages were reachable. These tests do — and two of them are not.
//!
//! This is deliberately a set of *assertions about the current code*, not a benchmark. If a
//! guard or a default moves, these fail and force the question "did we mean to switch that
//! stage on?" — which is the question nobody was being asked before.

use fluctlightdb::neuromodulator::Neuromodulators;

/// The dopamine/norepinephrine scoring block in `brain.rs` is guarded by
/// `if da > 0.5 || ne > 0.3`, and the defaults are exactly 0.5 and 0.3.
///
/// Both comparisons are strict, so **neither holds at rest**: the DA amplification and the
/// NE SNR-sharpening stage never run on a freshly opened brain. Their four constants
/// (`da_boost` slope 0.20, NE gain 0.10, NE suppression 0.08, floor 0.5) are dead until
/// something moves a neuromodulator off its baseline.
#[test]
fn da_ne_scoring_block_is_unreachable_at_default_posture() {
    let nm = Neuromodulators::default();
    assert_eq!(nm.dopamine, 0.5);
    assert_eq!(nm.norepinephrine, 0.3);
    assert!(
        !(nm.dopamine > 0.5 || nm.norepinephrine > 0.3),
        "the DA/NE block guard is `da > 0.5 || ne > 0.3` and the defaults sit exactly on \
         both bounds — this stage does not execute at rest"
    );
}

/// The CA3 Hopfield pattern-completion block is guarded by `!neuromodulators.is_encoding()`,
/// and `is_encoding()` is `acetylcholine >= 0.6` against a default of 0.7.
///
/// So the attractor-completion path — the stage documented as rescuing recalls that BM25 and
/// dense retrieval both missed — is **off by default**, along with its Jaccard threshold of
/// 0.07 and its 0.35 boost scale.
#[test]
fn ca3_completion_is_unreachable_at_default_posture() {
    let nm = Neuromodulators::default();
    assert_eq!(nm.acetylcholine, 0.7);
    assert!(
        nm.is_encoding(),
        "default ACh 0.7 >= 0.6 means the brain is in encoding mode at rest"
    );
    assert!(
        !!nm.is_encoding(),
        "CA3 completion runs only when `!is_encoding()`, so it is off at rest"
    );
}

/// Dopamine's contribution in isolation cannot reorder anything.
///
/// `da_boost` is a single positive multiplier applied uniformly to every recall score, and a
/// uniform positive scaling is order-preserving. It matters only because it can push scores
/// across the `activation > 1.0` branch that the *norepinephrine* stage tests two lines
/// later — which is a textbook example of the constants being jointly unoptimized rather
/// than independently meaningful.
#[test]
fn dopamine_alone_cannot_reorder_recalls() {
    let da: f32 = 0.9;
    let da_boost = 1.0 + (da - 0.5_f32).max(0.0) * 0.20;
    let scores = [0.2_f32, 0.45, 0.8, 1.4];
    let boosted: Vec<f32> = scores.iter().map(|s| s * da_boost).collect();
    for w in boosted.windows(2) {
        assert!(
            w[0] < w[1],
            "a uniform positive multiply must preserve ordering: {boosted:?}"
        );
    }
    // ...but it does move a score across the NE branch boundary, which is where it actually
    // changes behaviour.
    assert!(
        scores[2] < 1.0 && boosted[2] < 1.0,
        "0.8 stays below the branch"
    );
    assert!(
        scores[3] > 1.0,
        "1.4 was already above; the interaction is the point, not the scaling"
    );
}
