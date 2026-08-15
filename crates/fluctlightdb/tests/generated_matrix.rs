//! Production-certification matrix.
//!
//! This file previously held 250 `#[test]` functions which, once numeric suffixes were
//! normalized away, contained only **17 distinct bodies** — three shapes repeated fifty
//! times each plus fourteen `WiringBudget` variants. That was 52% of the suite's headline
//! test count carrying no additional coverage.
//!
//! It is now the same properties expressed as loops over their *full* argument domains,
//! which is strictly more coverage in a fraction of the lines: every `DevStage` rather than
//! an arbitrary sample, every budget invariant rather than two of them, plus monotonicity
//! and ordering properties the generated form never checked.

use fluctlightdb::auth::Role;
use fluctlightdb::budget::WiringBudget;
use fluctlightdb::development::DevStage;
use fluctlightdb::graph::BrainGraph;
use fluctlightdb::id::NeuronId;
use fluctlightdb::metrics::Metrics;
use fluctlightdb::plasticity::Synapse;
use fluctlightdb::tokenize::tokenize_rich;
use fluctlightdb::types::Region;

/// Every developmental stage, not the subset the generated file happened to sample.
const ALL_STAGES: &[DevStage] = &[
    DevStage::Embryonic,
    DevStage::Newborn,
    DevStage::Infant,
    DevStage::Child,
    DevStage::Adolescent,
    DevStage::Adult,
    DevStage::Expert,
];

const ALL_REGIONS: &[Region] = &[
    Region::Prefrontal,
    Region::HippocampusDg,
    Region::HippocampusCa3,
    Region::HippocampusCa1,
    Region::Amygdala,
    Region::Cortex,
    Region::Brainstem,
];

#[test]
fn wiring_budget_invariants_hold_for_every_stage() {
    for &stage in ALL_STAGES {
        let b = WiringBudget::for_stage(stage);
        assert!(
            b.max_ca3_clique_neighbors >= 2,
            "{stage:?}: a clique needs at least two neighbours"
        );
        assert!(
            b.max_dg_to_ec_links >= b.max_dg_chain_links,
            "{stage:?}: DG->EC fan-out must not be tighter than the DG chain feeding it"
        );
        assert!(
            b.max_dg_chain_links > 0 && b.max_ca3_chain_links > 0,
            "{stage:?}: a zero chain budget silently disables wiring"
        );
        assert!(
            b.max_semantic_ec_links > 0,
            "{stage:?}: semantic EC links must be reachable"
        );
    }
}

/// Budgets must never regress as the brain matures — a later stage wiring *less* than an
/// earlier one would mean growth shrinks capacity. The generated tests never checked this.
#[test]
fn wiring_budgets_do_not_regress_with_maturity() {
    for pair in ALL_STAGES.windows(2) {
        let (a, b) = (
            WiringBudget::for_stage(pair[0]),
            WiringBudget::for_stage(pair[1]),
        );
        assert!(
            b.max_dg_chain_links >= a.max_dg_chain_links,
            "{:?} -> {:?}: DG chain budget regressed",
            pair[0],
            pair[1]
        );
        assert!(
            b.max_ca3_clique_neighbors >= a.max_ca3_clique_neighbors,
            "{:?} -> {:?}: CA3 clique budget regressed",
            pair[0],
            pair[1]
        );
    }
}

#[test]
fn synapse_dedup_keeps_the_strongest_weight() {
    for (i, &region) in ALL_REGIONS.iter().enumerate() {
        let mut g = BrainGraph::default();
        let a = NeuronId::from_token(&format!("tok{i}a"));
        let b = NeuronId::from_token(&format!("tok{i}b"));
        g.add_synapse(Synapse::new(a, b, region, 0.4));
        g.add_synapse(Synapse::new(a, b, region, 0.9));
        assert_eq!(
            g.synapse_count(),
            1,
            "{region:?}: duplicate edge was not merged"
        );
        assert!(
            (g.synapses[0].weight - 0.9).abs() < 1e-5,
            "{region:?}: dedup must keep the stronger weight"
        );
        // A weaker later write must not overwrite the stronger stored weight.
        g.add_synapse(Synapse::new(a, b, region, 0.1));
        assert!(
            (g.synapses[0].weight - 0.9).abs() < 1e-5,
            "{region:?}: a weaker duplicate must not downgrade the edge"
        );
    }
}

#[test]
fn tokenize_rich_produces_usable_tokens_across_input_shapes() {
    let cases = [
        ("generated test sentence for matrix", "ctx", None),
        ("single", "c", None),
        ("with an outcome attached", "ctx", Some("saved")),
        ("punctuation, and; symbols! here?", "ctx", None),
        ("MiXeD CaSe InPuT", "CTX", None),
        ("unicode héllo 世界 memory", "ctx", None),
        ("   leading and trailing   ", "ctx", None),
    ];
    for (content, ctx, outcome) in cases {
        let tokens = tokenize_rich(content, ctx, outcome);
        assert!(!tokens.is_empty(), "no tokens produced for {content:?}");
        assert!(
            tokens.iter().all(|t| !t.surface.is_empty()),
            "empty surface form produced for {content:?}"
        );
    }
}

#[test]
fn role_ordering_is_total_and_consistent() {
    let ordered = [Role::Read, Role::Write, Role::Admin];
    for (i, &hi) in ordered.iter().enumerate() {
        for (j, &lo) in ordered.iter().enumerate() {
            assert_eq!(
                hi.allows(lo),
                i >= j,
                "{hi:?}.allows({lo:?}) should be {}",
                i >= j
            );
        }
    }
}

#[test]
fn metrics_render_prometheus_exposition() {
    let m = Metrics::new();
    for n in [1u64, 2, 10, 1000] {
        m.record_activate(n);
    }
    let out = m.render_prometheus();
    assert!(out.contains("fluctlight_activates_total"));
    assert!(
        out.lines().all(|l| !l.starts_with(' ')),
        "prometheus exposition must not have leading whitespace"
    );
}
