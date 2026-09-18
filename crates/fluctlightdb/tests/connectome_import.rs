//! Connectome segment persistence, projection wiring, and CSV import.

use fluctlightdb::connectome::{ConnectomeMeta, NeuronMeta, Nt};
use fluctlightdb::id::NeuronId;
use fluctlightdb::test_env::EnvGuard;
use fluctlightdb::FluctlightBrain;
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
