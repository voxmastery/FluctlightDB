//! Scale and separation quality benchmarks.

use fluctlightdb::{Episode, FluctlightBrain};

#[test]
fn scale_many_experiences_and_activate() {
    let mut brain = FluctlightBrain::new();
    for i in 0..200 {
        brain
            .experience(Episode {
                content: format!("scale event number {i} about caching"),
                context: "benchmark".into(),
                outcome: None,
                salience_hint: 0.4,
                semantic_vector: Some(vec![(i as f32) * 0.001, 0.5, 0.2]),
                agent_id: None,
                tenant_id: None,
                rag: None,
                provenance: None,
            })
            .unwrap();
    }
    assert!(brain.graph.synapse_count() > 500);
    let recalls = brain.activate("caching event");
    assert!(!recalls.recalls.is_empty());
}

#[test]
fn separation_quality_similar_events_stay_distinct() {
    let mut brain = FluctlightBrain::new();
    let a = brain
        .experience(Episode {
            content: "payment webhook timeout".into(),
            context: "prod".into(),
            outcome: None,
            salience_hint: 0.6,
            semantic_vector: Some(vec![1.0, 0.0, 0.0]),
            agent_id: None,
            tenant_id: None,
            rag: None,
            provenance: None,
        })
        .unwrap();
    let b = brain
        .experience(Episode {
            content: "payment webhook retry".into(),
            context: "prod".into(),
            outcome: None,
            salience_hint: 0.6,
            semantic_vector: Some(vec![0.95, 0.05, 0.0]),
            agent_id: None,
            tenant_id: None,
            rag: None,
            provenance: None,
        })
        .unwrap();
    assert!(b.separation.separation_index >= a.separation.separation_index * 0.5);
    assert_ne!(a.engram_id, b.engram_id);
}

#[test]
fn semantic_recall_crosses_lexical_gap() {
    let mut brain = FluctlightBrain::new();
    brain
        .experience(Episode {
            content: "redis cache invalidation race".into(),
            context: "debug".into(),
            outcome: None,
            salience_hint: 0.8,
            semantic_vector: Some(vec![0.9, 0.1, 0.0]),
            agent_id: None,
            tenant_id: None,
            rag: None,
            provenance: None,
        })
        .unwrap();
    let cue = vec![0.88, 0.12, 0.0];
    let result = brain.activate_with_semantic("memory store bug", Some(&cue));
    assert!(!result.recalls.is_empty());
}

#[test]
fn compact_reduces_duplicates() {
    let mut brain = FluctlightBrain::new();
    let ep = Episode {
        content: "duplicate fact".into(),
        context: "same".into(),
        outcome: None,
        salience_hint: 0.5,
        semantic_vector: None,
        agent_id: None,
        tenant_id: None,
        rag: None,
        provenance: None,
    };
    brain.experience(ep.clone()).unwrap();
    let second = brain.experience(ep).unwrap();
    if second.gate_rejected {
        assert_eq!(brain.hippocampus.engrams.len(), 1);
        return;
    }
    let before = brain.hippocampus.engrams.len();
    let report = brain.compact().unwrap();
    assert!(report.merged_engrams >= 1 || brain.hippocampus.engrams.len() < before);
}
