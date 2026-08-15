//! Somnus: always-on autonomic durability; seals must not change recall quality.

use fluctlightdb::test_env::EnvGuard;
use fluctlightdb::{Episode, FluctlightBrain};
use std::fs;
use tempfile::tempdir;

fn count_gens(path: &std::path::Path) -> usize {
    fs::read_dir(path.join("generations")).unwrap().count()
}

#[test]
fn somnus_on_without_user_env() {
    let _guard = EnvGuard::acquire(&[
        "FLUCTLIGHT_SOMNUS",
        "FLUCTLIGHT_SOMNUS_KEEP",
        "FLUCTLIGHT_SOMNUS_SEAL_EVERY_TICKS",
        "FLUCTLIGHT_CHECKPOINT_EVERY_N",
        "FLUCTLIGHT_STORAGE",
    ]);
    // User never "turns Somnus on" — env unset ⇒ enabled.
    std::env::remove_var("FLUCTLIGHT_SOMNUS");
    std::env::set_var("FLUCTLIGHT_SOMNUS_SEAL_EVERY_TICKS", "0"); // isolate wake test
    std::env::set_var("FLUCTLIGHT_CHECKPOINT_EVERY_N", "1");
    std::env::set_var("FLUCTLIGHT_STORAGE", "v4");

    assert!(fluctlightdb::somnus::somnus_enabled());

    let dir = tempdir().unwrap();
    let path = dir.path().join("brain");
    let mut brain = FluctlightBrain::open(&path).unwrap();
    brain.systems_seal().unwrap();
    let before = count_gens(&path);

    for _ in 0..20 {
        brain.tick().unwrap();
        brain.experience(Episode {
            content: "wake trace".into(),
            context: "somnus".into(),
            outcome: None,
            salience_hint: 0.4,
            semantic_vector: None,
            agent_id: None,
            tenant_id: None,
            rag: None,
            provenance: None,
        })
        .unwrap();
    }
    assert_eq!(
        count_gens(&path),
        before,
        "wake ticks/experiences must not mint seals; autonomic seal interval disabled"
    );
}

#[test]
fn somnus_autonomic_seal_runs_without_semantic_sleep() {
    let _guard = EnvGuard::acquire(&[
        "FLUCTLIGHT_SOMNUS",
        "FLUCTLIGHT_SOMNUS_KEEP",
        "FLUCTLIGHT_SOMNUS_SEAL_EVERY_TICKS",
        "FLUCTLIGHT_STORAGE",
    ]);
    std::env::remove_var("FLUCTLIGHT_SOMNUS"); // always-on, no user toggle
    std::env::set_var("FLUCTLIGHT_SOMNUS_KEEP", "5");
    std::env::set_var("FLUCTLIGHT_SOMNUS_SEAL_EVERY_TICKS", "5");
    std::env::set_var("FLUCTLIGHT_STORAGE", "v4");

    let dir = tempdir().unwrap();
    let path = dir.path().join("brain");
    let mut brain = FluctlightBrain::open(&path).unwrap();
    // Disable semantic auto-sleep so only Somnus durability can seal.
    brain.autonomic.config.auto_sleep = false;
    brain
        .experience(Episode {
            content: "before autonomic seal".into(),
            context: "somnus".into(),
            outcome: None,
            salience_hint: 0.5,
            semantic_vector: None,
            agent_id: None,
            tenant_id: None,
            rag: None,
            provenance: None,
        })
        .unwrap();
    brain.systems_seal().unwrap();
    let current_before = fs::read_to_string(path.join("CURRENT")).unwrap();

    for _ in 0..4 {
        brain.tick().unwrap();
    }
    let current_mid = fs::read_to_string(path.join("CURRENT")).unwrap();
    assert_eq!(
        current_before.trim(),
        current_mid.trim(),
        "must not seal before seal-every ticks"
    );

    brain.tick().unwrap(); // 5th ⇒ autonomic systems_seal
    let current_after = fs::read_to_string(path.join("CURRENT")).unwrap();
    assert_ne!(
        current_before.trim(),
        current_after.trim(),
        "autonomic Somnus must systems_seal on its own without sleep()"
    );
}

#[test]
fn somnus_systems_seal_does_not_change_activate_ranking() {
    let _guard = EnvGuard::acquire(&[
        "FLUCTLIGHT_SOMNUS",
        "FLUCTLIGHT_SOMNUS_SEAL_EVERY_TICKS",
        "FLUCTLIGHT_STORAGE",
    ]);
    std::env::remove_var("FLUCTLIGHT_SOMNUS");
    std::env::set_var("FLUCTLIGHT_SOMNUS_SEAL_EVERY_TICKS", "0");
    std::env::set_var("FLUCTLIGHT_STORAGE", "v4");

    let dir = tempdir().unwrap();
    let path = dir.path().join("brain");
    let mut brain = FluctlightBrain::open(&path).unwrap();
    for (i, content) in [
        "alpha preference dark mode",
        "beta shipping address line",
        "gamma pytest for unit tests",
    ]
    .iter()
    .enumerate()
    {
        brain
            .experience(Episode {
                content: (*content).into(),
                context: format!("q{i}"),
                outcome: None,
                salience_hint: 0.7,
                semantic_vector: None,
                agent_id: None,
                tenant_id: None,
                rag: None,
                provenance: None,
            })
            .unwrap();
    }

    let before: Vec<_> = brain
        .activate("pytest unit")
        .recalls
        .iter()
        .map(|r| r.engram_id)
        .collect();
    let synapses_before = brain.graph.synapse_count();

    brain.systems_seal().unwrap();

    let after: Vec<_> = brain
        .activate("pytest unit")
        .recalls
        .iter()
        .map(|r| r.engram_id)
        .collect();
    assert_eq!(
        before, after,
        "systems_seal must not change activate ranking (no quality tradeoff)"
    );
    assert_eq!(
        synapses_before,
        brain.graph.synapse_count(),
        "systems_seal must not prune synapses"
    );
}

#[test]
fn somnus_sleep_seals_and_prunes_generations() {
    let _guard = EnvGuard::acquire(&[
        "FLUCTLIGHT_SOMNUS",
        "FLUCTLIGHT_SOMNUS_KEEP",
        "FLUCTLIGHT_SOMNUS_SEAL_EVERY_TICKS",
        "FLUCTLIGHT_STORAGE",
    ]);
    std::env::remove_var("FLUCTLIGHT_SOMNUS");
    std::env::set_var("FLUCTLIGHT_SOMNUS_KEEP", "2");
    std::env::set_var("FLUCTLIGHT_SOMNUS_SEAL_EVERY_TICKS", "0");
    std::env::set_var("FLUCTLIGHT_STORAGE", "v4");

    let dir = tempdir().unwrap();
    let path = dir.path().join("brain");
    let mut brain = FluctlightBrain::open(&path).unwrap();
    for i in 0..5 {
        brain
            .experience(Episode {
                content: format!("sleep seal {i}"),
                context: "somnus".into(),
                outcome: None,
                salience_hint: 0.6,
                semantic_vector: None,
                agent_id: None,
                tenant_id: None,
                rag: None,
                provenance: None,
            })
            .unwrap();
        brain.sleep().unwrap();
    }
    let count = count_gens(&path);
    assert!(
        count <= 2,
        "after repeated sleeps Somnus must prune to keep≤2, got {count}"
    );
}
