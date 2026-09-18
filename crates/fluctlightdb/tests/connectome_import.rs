//! Connectome segment persistence, projection wiring, and CSV import.

use fluctlightdb::connectome::{ConnectomeMeta, NeuronMeta, Nt};
use fluctlightdb::id::NeuronId;
use fluctlightdb::test_env::EnvGuard;
use fluctlightdb::{import_connectome, FluctlightBrain, ImportConfig};
use std::path::PathBuf;
use tempfile::tempdir;

fn v4_env() -> EnvGuard {
    let g = EnvGuard::acquire(&["FLUCTLIGHT_STORAGE", "FLUCTLIGHT_SOMNUS"]);
    std::env::remove_var("FLUCTLIGHT_SOMNUS");
    std::env::set_var("FLUCTLIGHT_STORAGE", "v4");
    g
}

fn tiny_meta() -> ConnectomeMeta {
    let mut m = ConnectomeMeta {
        source: "unit".into(),
        imported_at: 42,
        p99_syn: 10.0,
        fanout: 3,
        entry_class: "Kenyon_Cell".into(),
        entry_set: vec![NeuronId(1001), NeuronId(1002)],
        neurons: Default::default(),
    };
    m.neurons.insert(
        NeuronId(4001),
        NeuronMeta { nt: Nt::Gaba, super_class: "central".into(), class: "APL".into(), side: "right".into(), neuropil: "MB".into() },
    );
    m
}

#[test]
fn connectome_segment_round_trips_and_is_absent_by_default() {
    let _g = v4_env();
    let dir = tempdir().unwrap();
    let path = dir.path().join("brain");
    {
        let brain = FluctlightBrain::open(&path).unwrap();
        assert!(brain.connectome.is_none(), "fresh brain must have no connectome");
    }
    {
        let mut brain = FluctlightBrain::open(&path).unwrap();
        brain.connectome = Some(tiny_meta());
        brain.checkpoint().unwrap();
    }
    let brain = FluctlightBrain::open(&path).unwrap();
    assert_eq!(brain.connectome, Some(tiny_meta()));
}

#[test]
fn activate_projects_tokens_onto_entry_set_only_when_connectome_present() {
    let _g = v4_env();
    let dir = tempdir().unwrap();
    let mut brain = FluctlightBrain::open(dir.path().join("brain")).unwrap();
    assert_eq!(brain.activate("hello world").connectome_seeds, None);

    brain.connectome = Some(tiny_meta()); // fanout 3
    let r = brain.activate("hello world");
    assert_eq!(r.connectome_seeds, Some(2 * 3), "2 tokens × fanout 3");
}

fn fixture(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/connectome").join(name)
}

fn fixture_cfg(replace: bool) -> ImportConfig {
    ImportConfig {
        connections: fixture("connections.csv"),
        classification: fixture("classification.csv"),
        neurons: fixture("neurons.csv"),
        entry_class: "Kenyon_Cell".into(),
        fanout: 7,
        replace,
    }
}

#[test]
fn import_fixture_aggregates_pairs_signs_edges_and_builds_entry_set() {
    let _g = v4_env();
    let dir = tempdir().unwrap();
    let mut brain = FluctlightBrain::open(dir.path().join("brain")).unwrap();
    let report = import_connectome(&mut brain, &fixture_cfg(false)).unwrap();

    assert_eq!(report.rows_read, 40);
    assert_eq!(report.rows_malformed, 1, "one 'notanumber' row");
    assert_eq!(report.pairs, 38, "39 good rows, one duplicate pair (2001->1001)");
    assert_eq!(brain.graph.synapse_count(), 38);
    assert_eq!(report.neurons, 20);
    assert_eq!(report.entry_set_len, 5);
    assert_eq!(report.inhibitory_synapses, 7, "5 GABA from 4001 + 2 GLUT from 3002");
    assert_eq!(report.p99_syn, 40.0);
    assert!(report.warnings.is_empty());

    let meta = brain.connectome.as_ref().expect("connectome set");
    assert_eq!(meta.entry_set, vec![NeuronId(1001), NeuronId(1002), NeuronId(1003), NeuronId(1004), NeuronId(1005)]);
    assert_eq!(meta.neurons[&NeuronId(4001)].nt, Nt::Gaba);
    assert_eq!(meta.neurons[&NeuronId(5004)].nt, Nt::Unknown);
    assert_eq!(meta.neurons[&NeuronId(1001)].class, "Kenyon_Cell");
    assert_eq!(meta.neurons[&NeuronId(1001)].side, "right");

    // Aggregated pair 2001->1001 has Σsyn = 10, which is > the single-row 2002->1002 (9).
    let w = |from: u64, to: u64| brain.graph.neighbors(NeuronId(from)).find(|(_, t)| *t == NeuronId(to)).map(|(s, _)| s.weight).unwrap();
    assert!(w(2001, 1001) > w(2002, 1002));
    assert!(w(4001, 1001) < 0.0, "GABA edge is negative");
    assert!(w(3002, 3001) < 0.0, "GLUT edge is negative");
    assert!(w(5001, 1001) > 0.0, "DA edge is positive");
    assert!(w(2005, 1005) <= 1.0);
}

#[test]
fn import_is_persisted_and_recall_reaches_fly_circuitry() {
    let _g = v4_env();
    let dir = tempdir().unwrap();
    let path = dir.path().join("brain");
    {
        let mut brain = FluctlightBrain::open(&path).unwrap();
        import_connectome(&mut brain, &fixture_cfg(false)).unwrap();
        brain.checkpoint().unwrap();
    }
    let brain = FluctlightBrain::open(&path).unwrap();
    assert_eq!(brain.graph.synapse_count(), 38);
    let r = brain.activate("odor");
    assert_eq!(r.connectome_seeds, Some(7));
    // Seeds land on Kenyon cells, which fan out to MBONs / APL — more than the 7 seeds stay active.
    assert!(r.active_neurons > 7, "spread through fly edges expected, got {}", r.active_neurons);
}

#[test]
fn import_refuses_second_import_unless_replace() {
    let _g = v4_env();
    let dir = tempdir().unwrap();
    let mut brain = FluctlightBrain::open(dir.path().join("brain")).unwrap();
    import_connectome(&mut brain, &fixture_cfg(false)).unwrap();
    let err = import_connectome(&mut brain, &fixture_cfg(false)).unwrap_err();
    assert!(err.to_string().contains("already"), "{err}");
    let report = import_connectome(&mut brain, &fixture_cfg(true)).unwrap();
    assert_eq!(report.pairs, 38);
    assert_eq!(brain.graph.synapse_count(), 38, "replace must not double the graph");
}

#[test]
fn import_warns_when_brain_already_holds_engrams() {
    use fluctlightdb::Episode;
    let _g = v4_env();
    let dir = tempdir().unwrap();
    let mut brain = FluctlightBrain::open(dir.path().join("brain")).unwrap();
    brain.experience(Episode::new("the harbor was quiet", "ctx", 0.5)).unwrap();
    let report = import_connectome(&mut brain, &fixture_cfg(false)).unwrap();
    assert_eq!(report.warnings.len(), 1);
    assert!(report.warnings[0].contains("engram"));
}

#[test]
fn import_rejects_header_mismatch_and_malformed_majority() {
    let _g = v4_env();
    let dir = tempdir().unwrap();
    let bad = dir.path().join("bad.csv");
    std::fs::write(&bad, "a,b,c,d,e\n1,2,x,3,ACH\n").unwrap();
    let mut cfg = fixture_cfg(false);
    cfg.connections = bad.clone();
    let mut brain = FluctlightBrain::open(dir.path().join("brain")).unwrap();
    let err = import_connectome(&mut brain, &cfg).unwrap_err();
    assert!(err.to_string().contains("pre_root_id"), "must name expected columns: {err}");

    std::fs::write(&bad, "pre_root_id,post_root_id,neuropil,syn_count,nt_type\n1,2,x,bad,ACH\n1,3,x,bad,ACH\n1,4,x,5,ACH\n").unwrap();
    let err = import_connectome(&mut brain, &cfg).unwrap_err();
    assert!(err.to_string().contains("malformed"), "{err}");
    assert!(brain.connectome.is_none(), "failed import must leave no partial state");
}

#[test]
fn sleep_cycle_leaves_inhibitory_synapses_untouched() {
    let _g = v4_env();
    let dir = tempdir().unwrap();
    let mut brain = FluctlightBrain::open(dir.path().join("brain")).unwrap();
    import_connectome(&mut brain, &fixture_cfg(false)).unwrap();
    let w = |b: &FluctlightBrain, from: u64, to: u64| b.graph.neighbors(NeuronId(from)).find(|(_, t)| *t == NeuronId(to)).map(|(s, _)| s.weight).unwrap();
    let before: Vec<f32> = [(4001, 1001), (4001, 1002), (4001, 1003), (4001, 1004), (4001, 1005), (3002, 3001), (3002, 3003)].iter().map(|&(f, t)| w(&brain, f, t)).collect();
    assert!(before.iter().all(|x| *x < 0.0));
    let count = brain.graph.synapse_count();
    brain.sleep().unwrap();
    let after: Vec<f32> = [(4001, 1001), (4001, 1002), (4001, 1003), (4001, 1004), (4001, 1005), (3002, 3001), (3002, 3003)].iter().map(|&(f, t)| w(&brain, f, t)).collect();
    assert_eq!(before, after, "sleep must not touch inhibitory weights");
    assert_eq!(brain.graph.synapse_count(), count, "sleep must not prune inhibitory synapses");
}

/// `nt_type_score` starts with `nt_type`: without the trailing comma in the expected prefix a
/// neurons.csv that has no `nt_type` column at all would pass the header check and silently
/// import a whole connectome with zero inhibition.
#[test]
fn import_rejects_neurons_header_without_nt_type_column() {
    let _g = v4_env();
    let dir = tempdir().unwrap();
    let bad = dir.path().join("neurons_bad.csv");
    std::fs::write(
        &bad,
        "root_id,group,nt_type_score,da_avg,ser_avg,gaba_avg,glut_avg,ach_avg,oct_avg\n1001,MB,0.9,0,0,0,0,0.9,0\n",
    )
    .unwrap();
    let mut cfg = fixture_cfg(false);
    cfg.neurons = bad;
    let mut brain = FluctlightBrain::open(dir.path().join("brain")).unwrap();
    let err = import_connectome(&mut brain, &cfg).unwrap_err();
    let msg = err.to_string();
    assert!(msg.contains("header mismatch"), "must be a header error: {msg}");
    assert!(msg.contains("nt_type,"), "must name the nt_type column: {msg}");
    assert!(brain.connectome.is_none(), "failed import must leave no partial state");
}

/// A raw dump must carry the `connectome` segment, not just the synapses: without it the
/// restored brain has the wiring but no entry set, so cues never reach the substrate.
#[test]
fn export_raw_import_raw_round_trips_the_connectome() {
    let _g = EnvGuard::acquire(&["FLUCTLIGHT_STORAGE", "FLUCTLIGHT_SOMNUS", "FLUCTLIGHT_EXPORT_SYNAPSES"]);
    std::env::remove_var("FLUCTLIGHT_SOMNUS");
    std::env::set_var("FLUCTLIGHT_STORAGE", "v4");
    std::env::set_var("FLUCTLIGHT_EXPORT_SYNAPSES", "1");

    let dir = tempdir().unwrap();
    let mut src = FluctlightBrain::open(dir.path().join("src")).unwrap();
    import_connectome(&mut src, &fixture_cfg(false)).unwrap();
    let dump = src.export_raw();
    assert!(dump.connectome.is_some(), "export must carry the connectome");

    let mut dst = FluctlightBrain::open(dir.path().join("dst")).unwrap();
    assert!(dst.connectome.is_none());
    let report = fluctlightdb::import_raw(&mut dst, dump).unwrap();
    assert_eq!(report.synapses, 38);
    let restored = dst.connectome.as_ref().expect("import must restore the connectome");
    assert_eq!(restored.entry_set.len(), 5);
    assert_eq!(dst.graph.synapse_count(), 38);
    assert_eq!(dst.activate("odor").connectome_seeds, Some(7), "the restored entry set must project");
}
