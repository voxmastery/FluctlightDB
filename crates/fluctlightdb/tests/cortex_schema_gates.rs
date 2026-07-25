//! Phase A CortexSchema gates — support integrity, persistence, crystallize, recombination.

use fluctlightdb::{Episode, FluctlightBrain, Schema};
use tempfile::tempdir;

#[test]
fn cortex_schemas_survive_checkpoint() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("brain");
    let mut brain = FluctlightBrain::open(&path).unwrap();
    let eid = brain
        .experience(Episode::new("User prefers dark mode", "prefs", 0.9))
        .unwrap()
        .engram_id;
    brain
        .cortex
        .schemas
        .upsert_active(Schema::new("user prefers dark mode", vec![eid]))
        .unwrap();
    brain.checkpoint().unwrap();
    drop(brain);
    let brain2 = FluctlightBrain::open(&path).unwrap();
    assert_eq!(brain2.cortex.schemas.active().count(), 1);
}
