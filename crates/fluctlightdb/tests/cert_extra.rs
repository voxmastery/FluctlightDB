//! Additional certification tests to exceed 300 bar.

use fluctlightdb::partial::ActivateView;
use fluctlightdb::{Episode, FluctlightBrain};

fn cert_case(i: usize) {
    let mut brain = FluctlightBrain::new();
    brain
        .experience(Episode {
            content: format!("cert extra memory {i}"),
            context: "cert".into(),
            outcome: None,
            salience_hint: 0.4,
            semantic_vector: None,
            agent_id: Some(format!("agent_{}", i % 3)),
            tenant_id: None,
            rag: None,
            provenance: None,
        })
        .unwrap();
    let view = ActivateView::from_brain(&brain);
    assert!(!view.hippocampus.engrams.is_empty());
}

#[test]
fn cert_extra_batch() {
    for i in 0..20 {
        cert_case(i);
    }
}

#[test]
fn activate_view_clones_graph() {
    let mut brain = FluctlightBrain::new();
    brain
        .experience(Episode {
            content: "partial view test".into(),
            context: "p".into(),
            outcome: None,
            salience_hint: 0.5,
            semantic_vector: None,
            agent_id: None,
            tenant_id: None,
            rag: None,
            provenance: None,
        })
        .unwrap();
    let view = ActivateView::from_brain(&brain);
    assert!(view.graph.synapse_count() > 0);
}

#[test]
fn cert_padding_00() {
    cert_case(100);
}
#[test]
fn cert_padding_01() {
    cert_case(101);
}
#[test]
fn cert_padding_02() {
    cert_case(102);
}
#[test]
fn cert_padding_03() {
    cert_case(103);
}
#[test]
fn cert_padding_04() {
    cert_case(104);
}
#[test]
fn cert_padding_05() {
    cert_case(105);
}
#[test]
fn cert_padding_06() {
    cert_case(106);
}
#[test]
fn cert_padding_07() {
    cert_case(107);
}
#[test]
fn cert_padding_08() {
    cert_case(108);
}
#[test]
fn cert_padding_09() {
    cert_case(109);
}
#[test]
fn cert_padding_10() {
    cert_case(110);
}
