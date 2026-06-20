//! Load smoke — many experiences + activate under budget.

use fluctlightdb::{Episode, FluctlightBrain};

fn load_n() -> usize {
    std::env::var("FLUCTLIGHT_LOAD_N")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(500)
}

#[test]
fn load_smoke_ten_k_experiences() {
    let n = load_n();
    let mut brain = FluctlightBrain::new();
    for i in 0..n {
        brain
            .experience(Episode {
                content: format!("load smoke event {i} about distributed cache layer"),
                context: "load".into(),
                outcome: None,
                salience_hint: 0.35,
                semantic_vector: None,
                agent_id: None,
                tenant_id: None,
                rag: None,
                provenance: None,
            })
            .unwrap();
    }
    assert!(brain.graph.synapse_count() > 100);
    let r = brain.activate("distributed cache");
    assert!(!r.recalls.is_empty());
}
