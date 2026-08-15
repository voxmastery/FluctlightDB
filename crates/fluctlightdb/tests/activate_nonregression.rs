//! Default activate() must not change when schemas are present.

use fluctlightdb::{Episode, FluctlightBrain, Schema};

#[test]
fn default_activate_unchanged_by_schemas_present() {
    let mut brain = FluctlightBrain::new();
    brain
        .experience(Episode::new("alpha wallet balance is 42", "ledger", 0.9))
        .unwrap();
    brain
        .experience(Episode::new("beta shipping address line", "ship", 0.7))
        .unwrap();
    let before: Vec<_> = brain
        .activate("wallet balance")
        .recalls
        .iter()
        .map(|r| r.engram_id)
        .collect();
    if let Some(eid) = before.first() {
        brain
            .cortex
            .schemas
            .upsert_active(Schema::new("wallet balance tracked", vec![*eid]))
            .unwrap();
    }
    let after: Vec<_> = brain
        .activate("wallet balance")
        .recalls
        .iter()
        .map(|r| r.engram_id)
        .collect();
    assert_eq!(before, after);
}
