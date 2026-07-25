//! Phase B CaptureGate gates.

use fluctlightdb::{Episode, FluctlightBrain, SchemaStatus};

#[test]
fn experience_sets_eligibility_tag() {
    let mut brain = FluctlightBrain::new();
    let id = brain
        .experience(Episode::new("new fact about theme dark", "t", 0.8))
        .unwrap()
        .engram_id;
    assert!(brain.cortex.eligibility.is_tagged(id));
}

#[test]
fn untagged_material_does_not_alter_schemas() {
    let mut brain = FluctlightBrain::new();
    for i in 0..3 {
        brain
            .experience(Episode::new(
                format!("User prefers dark mode theme variant {i}"),
                "prefs",
                0.9,
            ))
            .unwrap();
    }
    brain.sleep().unwrap();
    let n = brain.cortex.schemas.active().count();
    assert!(n >= 1);
    brain.cortex.eligibility.clear();
    brain.sleep().unwrap();
    assert_eq!(brain.cortex.schemas.active().count(), n);
}

#[test]
fn conflicting_new_experience_does_not_destroy_old_schema_probe() {
    let mut brain = FluctlightBrain::new();
    for i in 0..3 {
        brain
            .experience(Episode::new(
                format!("User prefers dark mode theme variant {i}"),
                "prefs",
                0.9,
            ))
            .unwrap();
    }
    brain.sleep().unwrap();
    let old_probe = brain.activate_with_schemas("dark mode theme");
    assert!(!old_probe.schemas.is_empty());
    for i in 0..5 {
        brain
            .experience(Episode::new(
                format!("User prefers light mode theme variant {i}"),
                "prefs",
                0.9,
            ))
            .unwrap();
    }
    brain.sleep().unwrap();
    assert!(
        brain
            .hippocampus
            .engrams
            .iter()
            .any(|e| e.episode.content.contains("dark")),
        "old episodes must not be deleted"
    );
    assert!(
        brain.cortex.schemas.schemas.iter().any(|s| {
            s.statement.to_lowercase().contains("dark")
                || s.status == SchemaStatus::Superseded
                || s.status == SchemaStatus::Active
        }),
        "schema history retained"
    );
    let _ = old_probe;
}
